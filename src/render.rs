use std::time::{Duration, Instant};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
                surface::render_elements_from_surface_tree,
                texture::{TextureBuffer, TextureRenderElement},
                Kind,
            },
            gles::{GlesRenderer, GlesTexture},
        },
        winit::{self, WinitEvent},
        SwapBuffersError,
    },
    desktop::{layer_map_for_output, space::space_render_elements},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Physical, Point, Rectangle, Size, Transform},
    wayland::shell::wlr_layer::Layer as WlrLayer,
};

use crate::cursor::{
    CursorRenderElement, CursorTextureElement,
    TontooRenderElements, WallpaperElement, WindowBorderElement,
    WindowShadowElement,
};
use crate::wallpaper::Wallpaper;
use crate::TontooCompositor;

pub fn init_winit(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init()?;

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "tontoo".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "TontooOS".into(),
            model: "TontooCompositor".into(),
            serial_number: "".into(),
        },
    );
    let _global = output.create_global::<TontooCompositor>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    let mut last_tick = Instant::now();

    event_loop
        .handle()
        .insert_source(winit, move |event, _, state| {
            match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => {
                    state.process_input_event(event);
                    // If animations are active, request continuous redraws
                    if state.animation_manager.has_active() {
                        backend.window().request_redraw();
                    }
                }
                WinitEvent::Redraw => {
                    // Tick animations
                    let now = Instant::now();
                    let dt = now.duration_since(last_tick);
                    last_tick = now;
                    state.animation_manager.tick(dt);
                    // Promote finished wallpaper fades before building elements.
                    state.finish_wallpaper_fade_if_done(now);
                    let fade_alpha = state.wallpaper_fade_alpha(now);

                    let size = backend.window_size();
                    let screen_w = size.w as f32;
                    let screen_h = size.h as f32;
                    let damage = Rectangle::from_size(size);

                    {
                        let (renderer, mut framebuffer) = backend.bind().unwrap();

                        let space_elements = space_render_elements(
                            renderer,
                            std::iter::once(&state.space),
                            &output,
                            1.0,
                        )
                        .map_err(|_| {
                            SwapBuffersError::ContextLost(Box::new(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Failed to get render elements",
                            )))
                        })
                        .unwrap();

                        // Wallpaper: use cached GPU buffer if available
                        let wallpaper = state.wallpaper.as_ref();
                        let wallpaper_buffer = &mut state.wallpaper_buffer;
                        let wallpaper_fade = state.wallpaper_fade.as_ref();
                        let wallpaper_fade_buffer = &mut state.wallpaper_fade_buffer;
                        let wallpaper_fill = state.wallpaper_fill.clone();
                        let output_size = Size::from((size.w as i32, size.h as i32));
                        let mut wallpaper_elements: Vec<TextureRenderElement<GlesTexture>> =
                            Vec::new();
                        if let Some(wp) = wallpaper {
                            if wallpaper_buffer.is_none() {
                                *wallpaper_buffer = create_winit_wallpaper_buffer(renderer, wp);
                            }
                            if let Some(buf) = wallpaper_buffer.as_ref() {
                                wallpaper_elements.extend(winit_wallpaper_elements(
                                    buf,
                                    wp,
                                    output_size,
                                    &wallpaper_fill,
                                    None,
                                ));
                            }
                        }
                        // Crossfade overlay: incoming wallpaper on top with eased alpha.
                        if let (Some(fade), Some(alpha)) = (wallpaper_fade, fade_alpha) {
                            if wallpaper_fade_buffer.is_none() {
                                *wallpaper_fade_buffer =
                                    create_winit_wallpaper_buffer(renderer, &fade.next);
                            }
                            if let Some(buf) = wallpaper_fade_buffer.as_ref() {
                                wallpaper_elements.extend(winit_wallpaper_elements(
                                    buf,
                                    &fade.next,
                                    output_size,
                                    &wallpaper_fill,
                                    Some(alpha),
                                ));
                            }
                        }

                        let pointer_pos = state.seat.get_pointer().map(|p| p.current_location());

                        // Get cursor element before building the list, to maintain z-order
                        let cursor_element = pointer_pos
                            .and_then(|pos| state.cursor.get_cursor_element(renderer, pos));

                        let (widget_cursor, surface_cursor) = match cursor_element {
                            Some(CursorRenderElement::Texture(e)) => (Some(e), None),
                            Some(CursorRenderElement::Surface(e)) => (None, Some(e)),
                            None => (None, None),
                        };

                        let mut all_elements: Vec<TontooRenderElements> =
                            Vec::with_capacity(space_elements.len() + 8);

                        // Z-order: Wallpaper → Window Shadows → Space → Window Borders → Window Controls → Cursor
                        // NOTE: no compositor-side menubar or dock. The top bar is the external
                        // Menubar.app system app and the bottom dock is the external Dock.app
                        // system app (both start via LaunchPad as layer-shell surfaces).

                        // Wallpaper (bottommost, then the crossfade overlay).
                        for wp in wallpaper_elements {
                            all_elements.push(TontooRenderElements::Wallpaper(WallpaperElement(wp)));
                        }

                        // Layer-shell surfaces below windows (Background/Bottom layers).
                        {
                            let map = layer_map_for_output(&output);
                            for layer_surface in map.layers() {
                                if !matches!(
                                    layer_surface.layer(),
                                    WlrLayer::Background | WlrLayer::Bottom
                                ) {
                                    continue;
                                }
                                let Some(geo) = map.layer_geometry(layer_surface) else {
                                    continue;
                                };
                                // CursorSurface is the generic wl_surface element variant.
                                let elems: Vec<TontooRenderElements> =
                                    render_elements_from_surface_tree(
                                        renderer,
                                        layer_surface.wl_surface(),
                                        Point::<i32, Physical>::from((geo.loc.x, geo.loc.y)),
                                        1.0,
                                        1.0,
                                        Kind::Unspecified,
                                    );
                                all_elements.extend(elems);
                            }
                        }

                        if false {
                            let blur = crate::render::WINDOW_SHADOW_BLUR;
                            let pad = blur as i32 + 4;
                            let offset_y = crate::render::WINDOW_SHADOW_OFFSET_Y;
                            for window in state.space.elements() {
                                if let Some(geo) = state.space.element_geometry(window) {
                                    let win_w = geo.size.w;
                                    let win_h = geo.size.h;
                                    // Shadow (cached)
                                    let shadow_key = (win_w, win_h, state.color_scheme);
                                    if !state.render_cache.window_shadows.contains_key(&shadow_key) {
                                        if let Some(buf) = create_window_shadow_texture(renderer, win_w, win_h, state.color_scheme) {
                                            state.render_cache.window_shadows.insert(shadow_key, buf);
                                        }
                                    }
                                    if let Some(ref shadow_buf) = state.render_cache.window_shadows.get(&shadow_key) {
                                        let shadow_pos = Point::from((
                                            (geo.loc.x - pad) as f64,
                                            (geo.loc.y as f64) - pad as f64 + offset_y,
                                        ));
                                        let shadow_size = Size::from((win_w + pad * 2, win_h + pad * 2));
                                        let elem = TextureRenderElement::from_texture_buffer(
                                            shadow_pos, shadow_buf, None, None,
                                            Some(shadow_size), Kind::Unspecified,
                                        );
                                        all_elements.push(TontooRenderElements::WindowShadow(WindowShadowElement(elem)));
                                    }
                                    // Border + rounded-corner mask (cached)
                                    let border_key = (win_w, win_h, state.color_scheme);
                                    if !state.render_cache.window_borders.contains_key(&border_key) {
                                        if let Some(buf) = create_window_border_mask_texture(renderer, win_w, win_h, state.color_scheme) {
                                            state.render_cache.window_borders.insert(border_key, buf);
                                        }
                                    }
                                    if let Some(ref border_buf) = state.render_cache.window_borders.get(&border_key) {
                                        let elem = TextureRenderElement::from_texture_buffer(
                                            Point::from((geo.loc.x as f64, geo.loc.y as f64)),
                                            border_buf, None, None,
                                            Some(Size::from((win_w, win_h))),
                                            Kind::Unspecified,
                                        );
                                        all_elements.push(TontooRenderElements::WindowBorder(WindowBorderElement(elem)));
                                    }
                                }
                            }
                        }

                        // Client windows
                        for elem in space_elements {
                            all_elements.push(TontooRenderElements::Space(elem));
                        }

                        // Server-side titlebars for SSD windows (Chrome/VSCode
                        // with system title bar). CSD windows draw their own
                        // header; maximized windows keep full content.
                        for window in state.space.elements() {
                            if !crate::shell::ssd::is_ssd(window) {
                                continue;
                            }
                            if crate::shell::ssd::is_maximized(window) {
                                continue;
                            }
                            if let Some(geo) = state.space.element_geometry(window) {
                                let title = crate::state::get_window_title(window);
                                crate::shell::ssd::push_ssd_elements(
                                    renderer,
                                    &mut state.render_cache,
                                    &mut state.shell.window_controls,
                                    state.focused_surface.as_ref(),
                                    state.color_scheme,
                                    window,
                                    geo,
                                    title,
                                    &mut all_elements,
                                );
                            }
                        }

                        // TontooUI surfaces (declarative widget-tree apps)
                        for surf in state.tontoo_ui.surfaces() {
                            if surf.parsed_tree.is_empty() { continue; }
                            let surf_w = surf.width as f32;
                            let surf_h = surf.height as f32;
                            let sx = (screen_w - surf_w) / 2.0;
                            let sy = (screen_h - surf_h) / 2.0;

                            // Window background
                            if let Some(glass) = &surf.glass {
                                if let Some(elem) = crate::widget_renderer::WidgetRenderer::render_glass_cmd(
                                    renderer, sx, sy, surf_w, surf_h, glass.milkiness, glass.alpha,
                                ) {
                                    all_elements.push(TontooRenderElements::TontooUi(
                                        crate::cursor::TontooUiTextureElement(elem)));
                                }
                            } else {
                                let bg = match surf.color_scheme {
                                    crate::handlers::tontoo_ui::TontooColorScheme::Dark =>
                                        crate::widget_renderer::Color::new(0.114, 0.114, 0.118, 1.0),
                                    crate::handlers::tontoo_ui::TontooColorScheme::Light =>
                                        crate::widget_renderer::Color::new(0.925, 0.925, 0.929, 1.0),
                                };
                                if let Some(elem) = crate::widget_renderer::WidgetRenderer::render_rect_cmd(
                                    renderer, sx, sy, surf_w, surf_h, bg,
                                ) {
                                    all_elements.push(TontooRenderElements::TontooUi(
                                        crate::cursor::TontooUiTextureElement(elem)));
                                }
                            }

                            // Widget content
                            let cmds = crate::widget_tree::widget_tree_to_draw_commands(
                                &surf.parsed_tree, sx, sy);
                            let elems = crate::widget_renderer::render_draw_commands_with(
                                renderer, &cmds, &mut state.widget_renderer);
                            for elem in elems {
                                all_elements.push(TontooRenderElements::TontooUi(
                                    crate::cursor::TontooUiTextureElement(elem)));
                            }
                        }

                        // No compositor-side dock: the bottom dock is the external
                        // Dock.app system app, drawn below as a layer-shell surface.

                        // Top strut: reserved for the external Menubar.app system
                        // app, drawn below as a Top-layer surface (same for the
                        // external Dock.app at the bottom).

                        // Layer-shell surfaces above windows (Top/Overlay
                        // layers, e.g. the Menubar top bar).
                        {
                            let map = layer_map_for_output(&output);
                            for layer_surface in map.layers() {
                                if !matches!(
                                    layer_surface.layer(),
                                    WlrLayer::Top | WlrLayer::Overlay
                                ) {
                                    continue;
                                }
                                let Some(geo) = map.layer_geometry(layer_surface) else {
                                    continue;
                                };
                                // CursorSurface is the generic wl_surface element variant.
                                let elems: Vec<TontooRenderElements> =
                                    render_elements_from_surface_tree(
                                        renderer,
                                        layer_surface.wl_surface(),
                                        Point::<i32, Physical>::from((geo.loc.x, geo.loc.y)),
                                        1.0,
                                        1.0,
                                        Kind::Unspecified,
                                    );
                                all_elements.extend(elems);
                            }
                        }

                        // Display overlays: brightness dim + night light
                        // warmth above content, below the cursor.
                        let display_brightness = state.display_brightness;
                        let display_night_light = state.display_night_light;
                        for elem in crate::display::overlay_elements(
                            renderer,
                            screen_w,
                            screen_h,
                            display_brightness,
                            display_night_light,
                        ) {
                            all_elements.push(TontooRenderElements::TontooUi(
                                crate::cursor::TontooUiTextureElement(elem)));
                        }

                        // Cursor rendered ABOVE all content
                        if let Some(e) = widget_cursor {
                            all_elements.push(TontooRenderElements::CursorTexture(CursorTextureElement(e)));
                        }

                        // Client-provided cursor surface rendered on top
                        if let Some(e) = surface_cursor {
                            all_elements.push(TontooRenderElements::CursorSurface(e));
                        }

                        damage_tracker
                            .render_output(
                                renderer,
                                &mut framebuffer,
                                0,
                                &all_elements,
                                state.color_scheme.clear_color(),
                            )
                            .unwrap();
                    }
                    backend.submit(Some(&[damage])).unwrap();

                    state.space.elements().for_each(|window| {
                        window.send_frame(
                            &output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        )
                    });

                    // Send frame callbacks to layer surfaces
                    let map = layer_map_for_output(&output);
                    for layer_surface in map.layers() {
                        layer_surface.send_frame(
                            &output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        );
                    }

                    state.space.refresh();
                    state.popups.cleanup();
                    let _ = state.display_handle.flush_clients();
                    // Keep frames coming until a wallpaper crossfade finishes.
                    if state.wallpaper_fade.is_some() {
                        backend.window().request_redraw();
                    }
                }
                WinitEvent::CloseRequested => {
                    state.loop_signal.stop();
                }
                _ => (),
            };
        })?;

    Ok(())
}

fn create_winit_wallpaper_buffer(
    renderer: &mut GlesRenderer,
    wallpaper: &Wallpaper,
) -> Option<TextureBuffer<GlesTexture>> {
    TextureBuffer::from_memory(
        renderer,
        wallpaper.pixels(),
        Fourcc::Abgr8888,
        wallpaper.size(),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

fn winit_wallpaper_elements(
    buffer: &TextureBuffer<GlesTexture>,
    wallpaper: &Wallpaper,
    output_size: Size<i32, Physical>,
    fill: &str,
    alpha: Option<f32>,
) -> Vec<TextureRenderElement<GlesTexture>> {
    let (wp_w, wp_h) = wallpaper.size();
    crate::wallpaper::wallpaper_layout(wp_w, wp_h, output_size.w, output_size.h, fill)
        .into_iter()
        .map(|quad| {
            TextureRenderElement::from_texture_buffer(
                Point::from(quad.offset),
                buffer,
                alpha,
                None,
                Some(Size::from(quad.size)),
                Kind::Unspecified,
            )
        })
        .collect()
}

// ── Rounded-rect helper (shared by window shadows / borders) ──

fn render_signed_dist_rounded(x: f64, y: f64, w: f64, h: f64, r: f64) -> f64 {
    let dx = x.max(r).min(w - r) - x;
    let dy = y.max(r).min(h - r) - y;
    (dx * dx + dy * dy).sqrt() - r
}

// ── Window decoration constants (macOS Tahoe 1:1) ──

pub const WINDOW_CORNER_RADIUS: f64 = 12.0;
pub const WINDOW_SHADOW_OFFSET_Y: f64 = 6.0;
pub const WINDOW_SHADOW_BLUR: f64 = 25.0;
pub const WINDOW_SHADOW_BASE_ALPHA_DARK: f64 = 0.20;
pub const WINDOW_SHADOW_BASE_ALPHA_LIGHT: f64 = 0.12;
pub const WINDOW_BORDER_WIDTH: f64 = 0.5;
// (Server-side titlebar textures live in `shell::ssd`, shared by both backends.)

/// Create a window shadow texture.
/// Returns a buffer of size (tex_w, tex_h) containing a gaussian-blurred
/// dark rounded-rect shadow centered in the texture.
pub fn create_window_shadow_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    win_h: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let cr = WINDOW_CORNER_RADIUS;
    let blur = WINDOW_SHADOW_BLUR;
    let pad = blur as i32 + 4;
    let tex_w = win_w + pad * 2;
    let tex_h = win_h + pad * 2;
    if tex_w <= 0 || tex_h <= 0 {
        return None;
    }
    let tw = tex_w as u32;
    let th = tex_h as u32;
    let mut data = vec![0u8; (tw * th * 4) as usize];

    let base_alpha = match color_scheme {
        crate::config::ColorScheme::Dark => WINDOW_SHADOW_BASE_ALPHA_DARK,
        crate::config::ColorScheme::Light => WINDOW_SHADOW_BASE_ALPHA_LIGHT,
    };

    let win_wf = win_w as f64;
    let win_hf = win_h as f64;
    let pad_f = pad as f64;

    for y in 0..th {
        for x in 0..tw {
            // Distance from the window's rounded-rect edge (window is centered in texture)
            let wx = x as f64 - pad_f;
            let wy = y as f64 - pad_f;
            let dist = render_signed_dist_rounded(wx, wy, win_wf, win_hf, cr);

            // Shadow: outside the window rect (dist > 0), gaussian falloff
            let shadow_alpha = if dist > 0.0 {
                let norm = dist / blur;
                let gauss = (-norm * norm * 0.5).exp();
                (gauss * base_alpha * 255.0).min(255.0) as u8
            } else {
                // Inside the window rect → transparent (shadow is behind window)
                0
            };

            let i = ((y * tw + x) * 4) as usize;
            data[i] = 0;     // B
            data[i + 1] = 0; // G
            data[i + 2] = 0; // R
            data[i + 3] = shadow_alpha;
        }
    }

    TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Abgr8888,
        (tex_w, tex_h),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

/// Create a window border + rounded-corner mask texture.
/// - Corners outside the rounded rect → filled with bg_color (masks the sharp window corners)
/// - 1px border stroke along the rounded rect edge
/// - Everything else → transparent (window content shows through)
pub fn create_window_border_mask_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    win_h: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let cr = WINDOW_CORNER_RADIUS;
    let bw = WINDOW_BORDER_WIDTH;
    let tw = win_w as u32;
    let th = win_h as u32;
    if tw == 0 || th == 0 {
        return None;
    }
    let mut data = vec![0u8; (tw * th * 4) as usize];

    // Background color for corner masking (matches clear_color)
    let (bg_r, bg_g, bg_b) = match color_scheme {
        crate::config::ColorScheme::Dark => (28u8, 28u8, 28u8),    // #1d1d1d
        crate::config::ColorScheme::Light => (236u8, 236u8, 236u8), // #ececec
    };
    // Border color (subtle shadow)
    let (bd_r, bd_g, bd_b, bd_a) = match color_scheme {
        crate::config::ColorScheme::Dark => (0u8, 0u8, 0u8, 60u8),     // rgba(0,0,0,0.24) subtle dark shadow
        crate::config::ColorScheme::Light => (0u8, 0u8, 0u8, 25u8),    // rgba(0,0,0,0.10) very subtle
    };

    for y in 0..th {
        for x in 0..tw {
            let dist = render_signed_dist_rounded(x as f64, y as f64, tw as f64, th as f64, cr);
            let i = ((y * tw + x) * 4) as usize;

            if dist > 0.0 {
                // Outside rounded rect → background color (mask the corner)
                data[i] = bg_r;
                data[i + 1] = bg_g;
                data[i + 2] = bg_b;
                data[i + 3] = 255;
            } else if dist > -bw {
                // Border stroke zone (within bw pixels of the edge)
                let edge_alpha = ((-dist) / bw).min(1.0);
                data[i] = bd_r;
                data[i + 1] = bd_g;
                data[i + 2] = bd_b;
                data[i + 3] = (bd_a as f64 * edge_alpha) as u8;
            } else {
                // Interior → transparent (window content visible)
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 0;
            }
        }
    }

    TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Abgr8888,
        (win_w, win_h),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

// (Server-side titlebar textures live in `shell::ssd`, shared by both backends.)
