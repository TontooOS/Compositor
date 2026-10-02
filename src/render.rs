use std::time::{Duration, Instant};
use std::{cell::RefCell, rc::Rc};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
                surface::render_elements_from_surface_tree,
                texture::{TextureBuffer, TextureRenderElement},
                Element, Kind,
            },
            gles::{GlesRenderer, GlesTexture},
        },
        winit::{self, WinitEvent},
        SwapBuffersError,
    },
    desktop::{layer_map_for_output, space::space_render_elements},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Logical, Physical, Point, Rectangle, Size, Transform},
    wayland::shell::wlr_layer::Layer as WlrLayer,
};

use crate::cursor::{
    CursorRenderElement, CursorTextureElement, TontooRenderElements, WallpaperElement,
};
use crate::wallpaper::Wallpaper;
use crate::TontooCompositor;

/// Winit backend shared between the winit event closure (which renders)
/// and the compositor state (which observes Wayland commits).
/// The winit backend only produces frames on OS `Redraw` events, so every
/// `pending_redraw` must be followed by an explicit
/// `window().request_redraw()` — otherwise mapped windows and committed
/// buffers never appear on screen until the next input event.
pub type SharedWinitBackend = Rc<
    RefCell<
        smithay::backend::winit::WinitGraphicsBackend<
            smithay::backend::renderer::gles::GlesRenderer,
        >,
    >,
>;

/// Request an OS redraw of the winit window when output is dirty.
/// Best-effort: if the backend is busy rendering (`try_borrow` fails),
/// the `pending_redraw` flag stays set and the event-loop idle hook
/// retries after the running frame.
pub fn kick_winit_redraw_if_dirty(state: &TontooCompositor) {
    if !state.pending_redraw {
        return;
    }
    if let Some(backend) = &state.winit_backend {
        if let Ok(backend) = backend.try_borrow() {
            backend.window().request_redraw();
        }
    }
}

pub fn init_winit(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>> {
    let (backend, winit) = winit::init()?;

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

    // Share the backend with the compositor state so Wayland commits, new
    // windows and IPC ops can kick OS redraws (see `SharedWinitBackend`).
    let backend: SharedWinitBackend = Rc::new(RefCell::new(backend));
    state.winit_backend = Some(backend.clone());

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
                    // The winit backend has no idle pump: drain
                    // `pending_redraw` here, otherwise cursor moves, new
                    // windows and client repaints never reach the screen
                    // until the next resize. Animations request continuous
                    // redraws below.
                    if state.pending_redraw || state.animation_manager.has_active() {
                        state.pending_redraw = false;
                        backend.borrow().window().request_redraw();
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

                    let size = backend.borrow().window_size();
                    let screen_w = size.w as f32;
                    let screen_h = size.h as f32;
                    let damage = Rectangle::from_size(size);
                    let frame_damage: Vec<Rectangle<i32, Physical>>;

                    {
                        let mut backend_ref = backend.borrow_mut();
                        let (renderer, mut framebuffer) = backend_ref.bind().unwrap();

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

                        // TEMP-DEBUG: invisible-window hunt (revert after diagnosis).
                        tracing::info!(
                            "TEMP-DEBUG winit frame: space_windows={} space_elems={}",
                            state.space.elements().count(),
                            space_elements.len(),
                        );
                        for w in state.space.elements() {
                            tracing::info!(
                                "TEMP-DEBUG window: loc={:?} geo={:?}",
                                state.space.element_location(w),
                                state.space.element_geometry(w),
                            );
                        }
                        for (i, elem) in space_elements.iter().enumerate() {
                            tracing::info!(
                                "TEMP-DEBUG space_elem[{}]: geo={:?} src={:?} kind={:?}",
                                i,
                                elem.geometry(smithay::utils::Scale::from(1.0)),
                                elem.src(),
                                elem.kind(),
                            );
                        }

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
                                // TEMP-DEBUG: wallpaper size bug hunt
                                tracing::info!(
                                    "wallpaper dbg: window={}x{} image={:?} fill={} buffer_ok={}",
                                    size.w,
                                    size.h,
                                    wp.size(),
                                    wallpaper_fill,
                                    wallpaper_buffer.is_some(),
                                );
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

                        // No compositor-side window shadow or border: apps and
                        // the GTK theme draw their own frame (see
                        // `create_window_shadow_texture`, kept for reference).

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

                            // Window background. The compositor never blurs:
                            // glass is a flat tint unless reduce transparency
                            // asks for a solid scheme color instead.
                            let use_tint = !state.accessibility.reduce_transparency;
                            if use_tint {
                                if let Some(glass) = &surf.glass {
                                    if let Some(elem) = crate::widget_renderer::WidgetRenderer::render_glass_cmd(
                                        renderer, sx, sy, surf_w, surf_h, glass.milkiness, glass.alpha,
                                    ) {
                                        all_elements.push(TontooRenderElements::TontooUi(
                                            crate::cursor::TontooUiTextureElement(elem)));
                                    }
                                }
                            }
                            if !use_tint || surf.glass.is_none() {
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

                        let result = damage_tracker
                            .render_output(
                                renderer,
                                &mut framebuffer,
                                0,
                                &all_elements,
                                state.color_scheme.clear_color(),
                            )
                            .unwrap();
                        frame_damage = result.damage.cloned().unwrap_or_default();
                        // Release the output frame so the renderer can draw
                        // the backdrop stream into its own target.
                        drop(framebuffer);

                        // Desktop backdrop stream: clients that render their
                        // own glass blur get the pixels below their window.
                        let wallpaper_for_backdrop = if let (Some(buf), Some(fade), Some(alpha)) =
                            (
                                state.wallpaper_fade_buffer.as_ref(),
                                state.wallpaper_fade.as_ref(),
                                fade_alpha,
                            ) {
                            Some((buf, &fade.next, state.wallpaper_fill.as_str(), Some(alpha)))
                        } else {
                            state
                                .wallpaper_buffer
                                .as_ref()
                                .zip(state.wallpaper.as_ref())
                                .map(|(buf, wp)| (buf, wp, state.wallpaper_fill.as_str(), None))
                        };
                        crate::backdrop::update_streams(
                            &mut state.tontoo_ui,
                            &state.space,
                            wallpaper_for_backdrop,
                            state.accessibility.reduce_transparency,
                            renderer,
                            &output,
                            Some(&frame_damage),
                            &mut state.backdrop_capture,
                            state.color_scheme.clear_color(),
                        );
                    }
                    backend.borrow_mut().submit(Some(&[damage])).unwrap();

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
                    // Frame produced: clear the dirty flag. Chain while
                    // animations or a wallpaper fade need continuous frames
                    // (winit has no frame timer of its own).
                    state.pending_redraw = false;
                    if state.animation_manager.has_active() || state.wallpaper_fade.is_some() {
                        backend.borrow().window().request_redraw();
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
        crate::wallpaper::GPU_UPLOAD_FLIPPED,
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
    // The src rect selects the sampled texture region in logical coords.
    // It must cover the whole texture: deriving it from the (differently
    // sized) dst quad samples out of bounds and clamps (mini image with
    // edge-stretched borders). The buffer was uploaded 1:1, so texture
    // logical size == pixel size.
    let src: Rectangle<f64, Logical> =
        Rectangle::from_size(Size::from((wp_w as f64, wp_h as f64)));
    let quads = crate::wallpaper::wallpaper_layout(wp_w, wp_h, output_size.w, output_size.h, fill);
    // TEMP-DEBUG: wallpaper size bug hunt
    for q in &quads {
        tracing::info!(
            "wallpaper dbg quad: out={}x{} img={}x{} offset={:?} size={:?}",
            output_size.w,
            output_size.h,
            wp_w,
            wp_h,
            q.offset,
            q.size,
        );
    }
    quads
        .into_iter()
        .map(|quad| {
            TextureRenderElement::from_texture_buffer(
                Point::from(quad.offset),
                buffer,
                alpha,
                Some(src),
                Some(Size::from(quad.size)),
                Kind::Unspecified,
            )
        })
        .collect()
}
// (Server-side titlebar textures live in shell::ssd, shared by both backends.)
