use std::time::{Duration, Instant};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
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
};

use crate::cursor::{
    CursorRenderElement, CursorTextureElement, DockBarElement,
    TontooRenderElements, WallpaperElement, WindowBorderElement, WindowControlsElement,
    WindowShadowElement, WindowTitlebarElement,
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

                    let size = backend.window_size();
                    let screen_w = size.w as f32;
                    let screen_h = size.h as f32;
                    let damage = Rectangle::from_size(size);

                    // Dock animation: tick spring physics, compute magnification
                    let dt_f32 = dt.as_secs_f32();
                    state.shell.dock.tick(dt_f32);
                    if let Some(pos) = state.seat.get_pointer().map(|p| p.current_location()) {
                        state
                            .shell
                            .dock
                            .compute_magnification(pos.x as f32, screen_w);
                    }

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
                        let wallpaper_element: Option<TextureRenderElement<GlesTexture>> =
                            wallpaper.and_then(|wp| {
                                let output_size = Size::from((size.w as i32, size.h as i32));
                                if wallpaper_buffer.is_none() {
                                    *wallpaper_buffer = create_winit_wallpaper_buffer(renderer, wp);
                                }
                                wallpaper_buffer
                                    .as_ref()
                                    .map(|buf| winit_wallpaper_from_buffer(buf, wp, output_size))
                            });

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

                        // Z-order: Wallpaper → Window Shadows → Space → Window Borders → Window Controls → Dock → Cursor
                        // NOTE: no compositor-side menubar. The top bar is the external
                        // Menubar.app system app (starts via LaunchPad, reserves 30px strut).

                        // Wallpaper (bottommost)
                        if let Some(wp) = wallpaper_element {
                            all_elements.push(TontooRenderElements::Wallpaper(WallpaperElement(wp)));
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

                        if false {
                            // Window titlebar panels disabled - now handled by GTK CSD
                            
                            for window in state.space.elements() {
                                if let Some(geo) = state.space.element_geometry(window) {
                                    let win_x = geo.loc.x as f32;
                                    let win_y = geo.loc.y as f32;
                                    let win_w = geo.size.w;
                                    let tb_h = crate::render::TITLEBAR_HEIGHT as f32;
                                    let tb_y = win_y - tb_h;

                                    // Draw glass titlebar background (per-width cache)
                                    let tb_key = (win_w, tb_h as i32, state.color_scheme);
                                    if !state.render_cache.window_titlebars.contains_key(&tb_key) {
                                        if let Some(buf) = create_window_titlebar_texture(renderer, win_w, state.color_scheme) {
                                            state.render_cache.window_titlebars.insert(tb_key, buf);
                                        }
                                    }
                                    if let Some(ref buf) = state.render_cache.window_titlebars.get(&tb_key) {
                                        let elem = TextureRenderElement::from_texture_buffer(
                                            Point::from((win_x as f64, tb_y as f64)),
                                            buf, None, None,
                                            Some(Size::from((win_w, tb_h as i32))),
                                            Kind::Unspecified,
                                        );
                                        all_elements.push(TontooRenderElements::WindowTitlebar(WindowTitlebarElement(elem)));
                                    }

                                    // Traffic light dots (close / minimize / maximize) — left side of titlebar
                                    {
                                        let tc_scale: i32 = 2;
                                        let dot_size = crate::shell::window_controls::DOT_SIZE as i32;
                                        let dot_spacing = crate::shell::window_controls::DOT_SPACING;
                                        let left_pad = crate::shell::window_controls::LEFT_PADDING;
                                        let top_pad = crate::shell::window_controls::TOP_PADDING;

                                        let is_focused = {
                                            let surface = window.toplevel().unwrap().wl_surface();
                                            state.focused_surface.as_ref() == Some(surface)
                                        };

                                        let window_id = format!("{}_{}", win_x as i32, win_y as i32);
                                        let is_hovered = state.shell.window_controls.get(&window_id)
                                            .map(|c| c.hovered).unwrap_or(false);

                                        let colors = if is_focused {
                                            [
                                                ("close".to_string(), crate::shell::window_controls::close_color(state.color_scheme)),
                                                ("minimize".to_string(), crate::shell::window_controls::minimize_color(state.color_scheme)),
                                                ("maximize".to_string(), crate::shell::window_controls::maximize_color(state.color_scheme)),
                                            ]
                                        } else {
                                            [
                                                ("close_inactive".to_string(), crate::shell::window_controls::close_color_inactive(state.color_scheme)),
                                                ("minimize_inactive".to_string(), crate::shell::window_controls::minimize_color_inactive(state.color_scheme)),
                                                ("maximize_inactive".to_string(), crate::shell::window_controls::maximize_color_inactive(state.color_scheme)),
                                            ]
                                        };

                                        let symbols = ['x', '-', '+'];

                                        for (i, (name, color)) in colors.iter().enumerate() {
                                            let dot_key = (name.clone(), dot_size * tc_scale, tc_scale, state.color_scheme);
                                            if !state.render_cache.traffic_light_dots.contains_key(&dot_key) {
                                                let pixel_data = crate::shell::window_controls::create_traffic_light_dot(
                                                    dot_size * tc_scale, *color,
                                                );
                                                if let Ok(buf) = TextureBuffer::from_memory(
                                                    renderer, &pixel_data, Fourcc::Abgr8888,
                                                    (dot_size * tc_scale, dot_size * tc_scale),
                                                    false, tc_scale, Transform::Normal, None,
                                                ) {
                                                    state.render_cache.traffic_light_dots.insert(dot_key.clone(), buf);
                                                }
                                            }
                                            if let Some(ref buf) = state.render_cache.traffic_light_dots.get(&dot_key) {
                                                let dot_x = win_x + left_pad + i as f32 * (dot_size as f32 + dot_spacing);
                                                let dot_y = tb_y + top_pad;
                                                let elem = TextureRenderElement::from_texture_buffer(
                                                    Point::from((dot_x as f64, dot_y as f64)),
                                                    &buf, None, None,
                                                    Some(Size::from((dot_size, dot_size))),
                                                    Kind::Unspecified,
                                                );
                                                all_elements.push(TontooRenderElements::WindowControls(WindowControlsElement(elem)));
                                            }

                                            // Hover symbol (× − +) overlay
                                            if is_hovered {
                                                let sym = symbols[i];
                                                let sym_name = format!("sym_{}_{}", name, sym);
                                                let sym_key = (sym_name.clone(), dot_size * tc_scale, tc_scale, state.color_scheme);
                                                if !state.render_cache.traffic_light_dots.contains_key(&sym_key) {
                                                    let pixel_data = crate::shell::window_controls::create_traffic_light_symbol(
                                                        dot_size * tc_scale, sym,
                                                    );
                                                    if let Ok(buf) = TextureBuffer::from_memory(
                                                        renderer, &pixel_data, Fourcc::Abgr8888,
                                                        (dot_size * tc_scale, dot_size * tc_scale),
                                                        false, tc_scale, Transform::Normal, None,
                                                    ) {
                                                        state.render_cache.traffic_light_dots.insert(sym_key.clone(), buf);
                                                    }
                                                }
                                                if let Some(ref buf) = state.render_cache.traffic_light_dots.get(&sym_key) {
                                                    let dot_x = win_x + left_pad + i as f32 * (dot_size as f32 + dot_spacing);
                                                    let dot_y = tb_y + top_pad;
                                                    let elem = TextureRenderElement::from_texture_buffer(
                                                        Point::from((dot_x as f64, dot_y as f64)),
                                                        &buf, None, None,
                                                        Some(Size::from((dot_size, dot_size))),
                                                        Kind::Unspecified,
                                                    );
                                                    all_elements.push(TontooRenderElements::WindowControls(WindowControlsElement(elem)));
                                                }
                                            }
                                        }
                                    }

                                    // Window title text — centered in titlebar
                                    {
                                        let title = crate::state::get_window_title(window)
                                            .unwrap_or_else(|| "TontooOS".to_string());

                                        let tb_text_color = if state.color_scheme == crate::config::ColorScheme::Dark {
                                            [255u8, 255, 255, 255]
                                        } else {
                                            [0u8, 0, 0, 255]
                                        };
                                        let font_size = 13.0;
                                        if let Some(text_buf) = render_text_texture(renderer, &title, font_size, tb_text_color, state.render_cache.font.as_ref()) {
                                            let text_w = (title.len() as f32 * font_size * 0.55) as f32;
                                            let center_x = win_x + (win_w as f32 - text_w) / 2.0;
                                            let text_y = tb_y + (tb_h - font_size) / 2.0;
                                            let elem = TextureRenderElement::from_texture_buffer(
                                                Point::from((center_x as f64, text_y as f64)),
                                                &text_buf, None, None, None,
                                                Kind::Unspecified,
                                            );
                                            all_elements.push(TontooRenderElements::WindowTitlebar(WindowTitlebarElement(elem)));
                                        }
                                    }
                                }
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

                        // Dock (glass panel + icons) — rendered at 2x for crisp HiDPI
                        {
                            const DOCK_H: f32 = 78.0;
                            const CORNER_R: f32 = 22.0;
                            const BOTTOM_MARGIN: f32 = 15.0;
                            const ICON_SIZE: f32 = 48.0;
                            const ICON_GAP: f32 = 12.0;
                            const ICON_RADIUS: f32 = 12.0;
                            const DOCK_SCALE: i32 = 2;

                            let sw = screen_w;
                            let sh = screen_h;
                            let icon_count = state.shell.dock.icons.len();
                            let total_icons_w = if icon_count > 0 {
                                ICON_SIZE * icon_count as f32 + ICON_GAP * (icon_count as f32 - 1.0).max(0.0)
                            } else {
                                0.0
                            };
                            let dw = (total_icons_w + 32.0).min(sw - 40.0);
                            let dx = (sw - dw) / 2.0;
                            let dy = sh - DOCK_H - BOTTOM_MARGIN;

                            // Glass panel (2x resolution, scale=2 for smithay)
                            let dock_color_scheme = state.color_scheme;
                            let dock_key = (dw as i32 * DOCK_SCALE, (DOCK_H * DOCK_SCALE as f32) as i32, dock_color_scheme);
                            if state.render_cache.dock_panel.as_ref().map(|(w2, h2, s2, _)| (*w2, *h2, *s2)) != Some(dock_key) {
                                let blur_data = state.wallpaper.as_ref().and_then(|wp| {
                                    let (wp_w, wp_h) = wp.size();
                                    Some((wp.pixels(), wp_w, wp_h, sw as i32, sh as i32, dx as i32, dy as i32))
                                });
                                if let Some(buf) = create_glass_texture(renderer, dw as i32 * DOCK_SCALE, (DOCK_H * DOCK_SCALE as f32) as i32, (CORNER_R * DOCK_SCALE as f32) as i32, blur_data, DOCK_SCALE) {
                                    state.render_cache.dock_panel = Some((dock_key.0, dock_key.1, dock_key.2, buf));
                                }
                            }
                            if let Some((_, _, _, ref buf)) = state.render_cache.dock_panel {
                                let elem = TextureRenderElement::from_texture_buffer(
                                    Point::from((dx as f64, dy as f64)),
                                    &buf, None, None,
                                    Some(Size::from((dw as i32, DOCK_H as i32))),
                                    Kind::Unspecified,
                                );
                                all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
                            }

                            // Icons (2x resolution, scale=2 for smithay)
                            let icon_start_x = dx + (dw - total_icons_w) / 2.0;
                            let icon_y = dy + (DOCK_H - ICON_SIZE) / 2.0;

                            for (i, icon) in state.shell.dock.icons.iter().enumerate() {
                                let (color, letter) = match icon.name.as_str() {
                                    "Finder" => (0xFF2196F3u32, 'F'),
                                    "Terminal" => (0xFF2979FFu32, 'T'),
                                    "Settings" => (0xFF9E9E9Eu32, 'S'),
                                    "Notes" => (0xFF4CAF50u32, 'N'),
                                    "Podcasts" => (0xFF9C27B0u32, 'P'),
                                    _ => (0xFF607D8Bu32, '?'),
                                };
                                let x = icon_start_x + i as f32 * (ICON_SIZE + ICON_GAP);

                                // Bounce offset
                                let bounce = state.shell.dock.bounce_offset(i);

                                // Hover gray border (2px)
                                let is_hovered = state.shell.dock.hover_index == Some(i);
                                if is_hovered {
                                    let border_w: f32 = 2.0;
                                    let hover_buf_key = format!("hover_border_{}_{}", ICON_SIZE as i32, ICON_RADIUS as i32);
                                    if !state.render_cache.dock_icons.contains_key(&(hover_buf_key.clone(), dock_color_scheme, (ICON_SIZE * DOCK_SCALE as f32) as i32)) {
                                        let s = (ICON_SIZE * DOCK_SCALE as f32) as i32;
                                        let cr = (ICON_RADIUS * DOCK_SCALE as f32) as i32;
                                        let bw = (border_w * DOCK_SCALE as f32) as i32;
                                        if let Some(buf) = create_hover_border(renderer, s, cr, bw, DOCK_SCALE) {
                                            state.render_cache.dock_icons.insert((hover_buf_key.clone(), dock_color_scheme, (ICON_SIZE * DOCK_SCALE as f32) as i32), buf);
                                        }
                                    }
                                    if let Some(ref buf) = state.render_cache.dock_icons.get(&(hover_buf_key, dock_color_scheme, (ICON_SIZE * DOCK_SCALE as f32) as i32)) {
                                        let elem = TextureRenderElement::from_texture_buffer(
                                            Point::from((x as f64, (icon_y + bounce) as f64)),
                                            &buf, None, None,
                                            Some(Size::from((ICON_SIZE as i32, ICON_SIZE as i32))),
                                            Kind::Unspecified,
                                        );
                                        all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
                                    }
                                }

                                // Icon texture
                                let icon_key = (icon.name.clone(), dock_color_scheme, (ICON_SIZE * DOCK_SCALE as f32) as i32);
                                if !state.render_cache.dock_icons.contains_key(&icon_key) {
                                    if let Some(buf) = create_dock_icon(renderer, (ICON_SIZE * DOCK_SCALE as f32) as i32, (ICON_RADIUS * DOCK_SCALE as f32) as i32, color, letter, DOCK_SCALE) {
                                        state.render_cache.dock_icons.insert(icon_key.clone(), buf);
                                    }
                                }
                                if let Some(ref buf) = state.render_cache.dock_icons.get(&icon_key) {
                                    let elem = TextureRenderElement::from_texture_buffer(
                                        Point::from((x as f64, (icon_y + bounce) as f64)),
                                        &buf, None, None,
                                        Some(Size::from((ICON_SIZE as i32, ICON_SIZE as i32))),
                                        Kind::Unspecified,
                                    );
                                    all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
                                }

                                // Running-app black dot indicator
                                if icon.is_running {
                                    let dot_size: f32 = 6.0;
                                    let dot_x = x + (ICON_SIZE - dot_size) / 2.0;
                                    let dot_y = icon_y + bounce + ICON_SIZE + 3.0;
                                    let dot_pixel: [u8; 4] = [0x00, 0x00, 0x00, 0xFF]; // black
                                    if let Ok(dot_buf) = TextureBuffer::from_memory(
                                        renderer, &dot_pixel, Fourcc::Abgr8888, (1, 1), false, 1, Transform::Normal, None,
                                    ) {
                                        let elem = TextureRenderElement::from_texture_buffer(
                                            Point::from((dot_x as f64, dot_y as f64)),
                                            &dot_buf, None, None,
                                            Some(Size::from((dot_size as i32, dot_size as i32))),
                                            Kind::Unspecified,
                                        );
                                        all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
                                    }
                                }
                            }
                        }

                        // Top strut: reserved for the external Menubar.app system app.
                        // The compositor renders nothing here; windows are placed
                        // below the strut (see handlers/xdg_shell.rs).

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

fn winit_wallpaper_from_buffer(
    buffer: &TextureBuffer<GlesTexture>,
    wallpaper: &Wallpaper,
    output_size: Size<i32, Physical>,
) -> TextureRenderElement<GlesTexture> {
    let (wp_w, wp_h) = wallpaper.size();
    let scale_x = output_size.w as f64 / wp_w as f64;
    let scale_y = output_size.h as f64 / wp_h as f64;
    let fill_scale = scale_x.max(scale_y);

    let scaled_w = (wp_w as f64 * fill_scale) as i32;
    let scaled_h = (wp_h as f64 * fill_scale) as i32;
    let offset_x = ((output_size.w - scaled_w) / 2) as f64;
    let offset_y = ((output_size.h - scaled_h) / 2) as f64;

    TextureRenderElement::from_texture_buffer(
        Point::from((offset_x, offset_y)),
        buffer,
        None,
        None,
        Some(Size::from((scaled_w, scaled_h))),
        Kind::Unspecified,
    )
}

// ── Dock helpers (duplicated from udev.rs for the winit backend) ──

fn create_glass_texture(
    renderer: &mut GlesRenderer,
    w: i32,
    h: i32,
    cr: i32,
    blur_src: Option<(&[u8], i32, i32, i32, i32, i32, i32)>,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>> {
    let wu = w as u32;
    let hu = h as u32;
    let mut data = vec![0u8; (wu * hu * 4) as usize];

    if let Some((wp_dat, wp_w, wp_h, sc_w, sc_h, dx, dy)) = blur_src {
        let fill_scale = (sc_w as f64 / wp_w as f64).max(sc_h as f64 / wp_h as f64);
        let off_x = ((sc_w as f64 - wp_w as f64 * fill_scale) / 2.0) as f64;
        let off_y = ((sc_h as f64 - wp_h as f64 * fill_scale) / 2.0) as f64;

        // 2-pass box blur (~10x10 Gaussian)
        // Map each texture pixel to the wallpaper: screen px = region origin + tex px / tex_scale,
        // wallpaper px = (screen px - wallpaper fill offset) / fill_scale.
        let mut tmp = vec![0u8; (wu * hu * 4) as usize];
        let tex_to_screen = 1.0 / tex_scale as f64;
        render_box_blur_5x5(
            wp_dat, &mut tmp, wu, hu, wp_w as u32, wp_h as u32,
            (dx as f64 - off_x) as i32, (dy as f64 - off_y) as i32,
            tex_to_screen, tex_to_screen, fill_scale,
        );
        render_box_blur_5x5(&tmp, &mut data, wu, hu, wu, hu, 0, 0, 0.0, 0.0, 1.0);

        // Glass overlay: 30% white tint over blurred wallpaper (matches vorschau alpha=0.32)
        for i in (0..data.len()).step_by(4) {
            data[i]   = (data[i] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i+1] = (data[i+1] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i+2] = (data[i+2] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i+3] = 255;
        }
    } else {
        for i in (0..data.len()).step_by(4) { data[i] = 255; data[i+1] = 255; data[i+2] = 255; data[i+3] = 77; }
    }

    for y in 0..hu { for x in 0..wu {
        let dist = render_signed_dist_rounded(x as f64, y as f64, wu as f64, hu as f64, cr as f64);
        let i = ((y * wu + x) * 4) as usize;

        if dist < 0.0 {
            // Inside rounded rect → keep glass content, full opacity
            data[i+3] = 255;
        } else if dist < 4.0 {
            // Border zone: 2px white border (4 texture px at 2× scale), fade out
            let border_t = dist; // 0..4
            let border_alpha = (160.0 * (1.0 - border_t / 4.0)) as u8; // 0.63 opacity, fade out
            let bg_r = data[i] as f32; let bg_g = data[i+1] as f32; let bg_b = data[i+2] as f32;
            let t = border_alpha as f32 / 255.0;
            data[i]   = (bg_r * (1.0 - t) + 255.0 * t) as u8;
            data[i+1] = (bg_g * (1.0 - t) + 255.0 * t) as u8;
            data[i+2] = (bg_b * (1.0 - t) + 255.0 * t) as u8;
            data[i+3] = 255;
        } else {
            // Outside → transparent
            data[i] = 0; data[i+1] = 0; data[i+2] = 0; data[i+3] = 0;
        }
    }}

    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (w, h), false, tex_scale, Transform::Normal, None).ok()
}

fn render_box_blur_5x5(src: &[u8], dest: &mut [u8], dw: u32, dh: u32, sw: u32, sh: u32, off_x: i32, off_y: i32, sx: f64, sy: f64, fill_scale: f64) {
    let ks: i32 = 2;
    for oy in 0..dh { for ox in 0..dw {
        let spx = if sx > 0.0 { ((ox as f64 * sx + off_x as f64) / fill_scale) as i32 } else { ox as i32 };
        let spy = if sy > 0.0 { ((oy as f64 * sy + off_y as f64) / fill_scale) as i32 } else { oy as i32 };
        let mut r = 0u32; let mut g = 0u32; let mut b = 0u32; let mut a = 0u32; let mut cnt = 0u32;
        for ky in -ks..=ks { for kx in -ks..=ks {
            let px = (spx + kx).clamp(0, sw as i32 - 1) as u32;
            let py = (spy + ky).clamp(0, sh as i32 - 1) as u32;
            let idx = ((py * sw + px) * 4) as usize;
            if idx + 3 < src.len() { r += src[idx] as u32; g += src[idx+1] as u32; b += src[idx+2] as u32; a += src[idx+3] as u32; cnt += 1; }
        }}
        let di = ((oy * dw + ox) * 4) as usize;
        if cnt > 0 { r /= cnt; g /= cnt; b /= cnt; a /= cnt; }
        dest[di] = r as u8; dest[di+1] = g as u8; dest[di+2] = b as u8; dest[di+3] = a as u8;
    }}
}

fn render_signed_dist_rounded(x: f64, y: f64, w: f64, h: f64, r: f64) -> f64 {
    let dx = x.max(r).min(w - r) - x;
    let dy = y.max(r).min(h - r) - y;
    (dx * dx + dy * dy).sqrt() - r
}

fn draw_letter_bitmap(data: &mut [u8], buf_w: u32, x: i32, y: i32, letter: char, color: u32) {
    let pattern: Vec<Vec<u8>> = match letter {
        'T' => vec![vec![1,1,1,1,1], vec![0,0,1,0,0], vec![0,0,1,0,0], vec![0,0,1,0,0], vec![0,0,1,0,0]],
        'N' => vec![vec![1,0,0,0,1], vec![1,1,0,0,1], vec![1,0,1,0,1], vec![1,0,0,1,1], vec![1,0,0,0,1]],
        'F' => vec![vec![1,1,1,1,1], vec![1,0,0,0,0], vec![1,1,1,0,0], vec![1,0,0,0,0], vec![1,0,0,0,0]],
        'S' => vec![vec![0,1,1,1,0], vec![1,0,0,0,0], vec![0,1,1,0,0], vec![0,0,0,1,0], vec![1,1,1,0,0]],
        'P' => vec![vec![1,1,1,0,0], vec![1,0,0,1,0], vec![1,1,1,0,0], vec![1,0,0,0,0], vec![1,0,0,0,0]],
        _ => return,
    };
    let scale = 4u32;
    let r = (color >> 16) as u8; let g = ((color >> 8) & 0xFF) as u8; let b = (color & 0xFF) as u8; let a = (color >> 24) as u8;
    for py in 0..pattern.len() {
        for px in 0..pattern[py].len() {
            if pattern[py][px] == 0 { continue; }
            for sy in 0..scale { for sx in 0..scale {
                let dx = (x as u32 + px as u32 * scale + sx) as u32;
                let dy = (y as u32 + py as u32 * scale + sy) as u32;
                let i = ((dy * buf_w + dx) * 4) as usize;
                if i + 3 < data.len() { data[i] = b; data[i + 1] = g; data[i + 2] = r; data[i + 3] = a; }
            }}
        }
    }
}

fn render_text_texture(
    renderer: &mut GlesRenderer,
    text: &str,
    font_size: f32,
    color: [u8; 4],
    font: Option<&fontdue::Font>,
) -> Option<TextureBuffer<GlesTexture>> {
    let font = font?;

    let px_size = font_size.max(1.0);
    let mut cursor_x: u32 = 0;
    let mut total_w: u32 = 0;
    let mut max_h: u32 = 0;

    struct Glyph { x: u32, width: u32, height: u32, bitmap: Vec<u8> }
    let mut glyphs: Vec<Glyph> = Vec::new();

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, px_size);
        let w = metrics.width as u32;
        let h = metrics.height as u32;
        if w > 0 && h > 0 {
            glyphs.push(Glyph { x: cursor_x, width: w, height: h, bitmap });
            total_w = total_w.max(cursor_x + w);
        }
        cursor_x += metrics.advance_width as u32;
        if h > max_h { max_h = h; }
    }

    let total_h = if max_h > 0 { max_h } else { px_size as u32 };
    if total_w == 0 || total_h == 0 { return None; }

    let mut rgba = vec![0u8; (total_w * total_h * 4) as usize];
    for g in &glyphs {
        for row in 0..g.height {
            for col in 0..g.width {
                let alpha = g.bitmap[(row * g.width + col) as usize];
                if alpha == 0 { continue };
                // Flip Y for OpenGL (texture Y=0 is bottom, bitmap Y=0 is top)
                let flipped_row = g.height - 1 - row;
                let px = ((flipped_row * total_w + g.x + col) * 4) as usize;
                if px + 3 < rgba.len() {
                    let a = ((alpha as u32 * color[3] as u32) / 255) as u8;
                    rgba[px] = (color[0] as u32 * a as u32 / 255) as u8;
                    rgba[px + 1] = (color[1] as u32 * a as u32 / 255) as u8;
                    rgba[px + 2] = (color[2] as u32 * a as u32 / 255) as u8;
                    rgba[px + 3] = a;
                }
            }
        }
    }

    TextureBuffer::from_memory(
        renderer, &rgba, Fourcc::Abgr8888,
        (total_w as i32, total_h as i32),
        false, 1, Transform::Normal, None,
    ).ok()
}

fn create_dock_icon(renderer: &mut GlesRenderer, size: i32, corner_radius: i32, color: u32, letter: char, tex_scale: i32) -> Option<TextureBuffer<GlesTexture>> {
    let su = size as u32; let cru = corner_radius as u32;
    let mut data = vec![0u8; (su * su * 4) as usize];
    for y in 0..su { for x in 0..su {
        let dist = if x < cru && y < cru { (((cru - x) * (cru - x) + (cru - y) * (cru - y)) as f64).sqrt() }
            else if x >= su - cru && y < cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y < cru { cru - y } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x < cru && y >= su - cru { let dx = if x < cru { cru - x } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x >= su - cru && y >= su - cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else { -1.0 };
        let i = ((y * su + x) * 4) as usize;
        if dist >= 0.0 && dist > cru as f64 { data[i + 3] = 0; }
        else { data[i] = (color & 0xFF) as u8; data[i + 1] = ((color >> 8) & 0xFF) as u8; data[i + 2] = ((color >> 16) & 0xFF) as u8; data[i + 3] = ((color >> 24) & 0xFF) as u8; }
    }}
    // Draw letter centered on icon
    draw_letter_bitmap(&mut data, su, ((su as i32 - 20) / 2).max(0), ((su as i32 - 20) / 2).max(0), letter, 0xFFFFFFFF);
    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (size, size), false, tex_scale, Transform::Normal, None).ok()
}

fn create_hover_border(
    renderer: &mut GlesRenderer,
    size: i32,
    corner_radius: i32,
    border_width: i32,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>> {
    let su = size as u32;
    let cru = corner_radius as u32;
    let bw = border_width as u32;
    let mut data = vec![0u8; (su * su * 4) as usize];

    for y in 0..su { for x in 0..su {
        let dist = if x < cru && y < cru { (((cru - x) * (cru - x) + (cru - y) * (cru - y)) as f64).sqrt() }
            else if x >= su - cru && y < cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y < cru { cru - y } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x < cru && y >= su - cru { let dx = if x < cru { cru - x } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x >= su - cru && y >= su - cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else { -1.0 };

        let i = ((y * su + x) * 4) as usize;

        // Border is drawn where dist is between 0 and bw (the rounded rect edge)
        if dist >= 0.0 && dist <= bw as f64 {
            // Anti-aliased inner edge (dist near 0) and outer edge (dist near bw)
            let inner_aa = if dist < 1.0 { dist } else { 1.0 };
            let outer_aa = if dist > bw as f64 - 1.0 { bw as f64 - dist } else { 1.0 };
            let alpha = (inner_aa * outer_aa * 160.0).min(255.0) as u8;
            data[i] = 180; data[i+1] = 180; data[i+2] = 180; data[i+3] = alpha;
        } else {
            data[i] = 0; data[i+1] = 0; data[i+2] = 0; data[i+3] = 0;
        }
    }}

    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (size, size), false, tex_scale, Transform::Normal, None).ok()
}

// ── Window decoration constants (macOS Tahoe 1:1) ──

pub const WINDOW_CORNER_RADIUS: f64 = 12.0;
pub const WINDOW_SHADOW_OFFSET_Y: f64 = 6.0;
pub const WINDOW_SHADOW_BLUR: f64 = 25.0;
pub const WINDOW_SHADOW_BASE_ALPHA_DARK: f64 = 0.20;
pub const WINDOW_SHADOW_BASE_ALPHA_LIGHT: f64 = 0.12;
pub const TITLEBAR_HEIGHT: i32 = crate::config::TITLEBAR_HEIGHT;
pub const WINDOW_BORDER_WIDTH: f64 = 0.5;

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

pub fn create_window_titlebar_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let tw = win_w as u32;
    let th = TITLEBAR_HEIGHT as u32;
    if tw == 0 {
        return None;
    }
    let mut data = vec![0u8; (tw * th * 4) as usize];

    // Solid titlebar — macOS Tahoe style, always #ececec
    let corner_r: f32 = 10.0;
    let (bg_r, bg_g, bg_b) = (236u8, 236u8, 236u8); // #ececec

    for y in 0..th {
        for x in 0..tw {
            let i = ((y * tw + x) * 4) as usize;
            // Rounded top corners
            let alpha = if (x as f32) < corner_r && (y as f32) < corner_r {
                let dx = corner_r - x as f32;
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r { 0.0 } else { 1.0 - (1.0 - dist / corner_r).powf(1.5) }
            } else if (x as f32) >= tw as f32 - corner_r && (y as f32) < corner_r {
                let dx = x as f32 - (tw as f32 - corner_r);
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r { 0.0 } else { 1.0 - (1.0 - dist / corner_r).powf(1.5) }
            } else {
                1.0
            };
            let a = (alpha * 255.0) as u8; // solid
            data[i] = bg_r;
            data[i + 1] = bg_g;
            data[i + 2] = bg_b;
            data[i + 3] = a;
        }
    }

    TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Abgr8888,
        (win_w, TITLEBAR_HEIGHT),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}
