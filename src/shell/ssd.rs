//! Server-side decorations (SSD) for foreign apps.
//!
//! GTK/Qt apps always use client-side decorations (CSD) and draw their own
//! MacTahoe header. Windows that negotiate `ServerSide` via xdg-decoration
//! (Chrome with "Use system title bar", VSCode with native title bar)
//! get a compositor-drawn traffic-light titlebar from this module.
//!
//! CSD windows are never touched here; see `window_controls` for the shared
//! traffic-light geometry used by both paths.

use std::collections::HashMap;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::texture::{TextureBuffer, TextureRenderElement},
            gles::{GlesRenderer, GlesTexture},
        },
    },
    desktop::Window,
    reexports::{
        wayland_protocols::xdg::{
            decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
            shell::server::xdg_toplevel::State as XdgState,
        },
        wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    },
    utils::{Logical, Point, Rectangle, Size, Transform},
};

use crate::{
    config::ColorScheme,
    cursor::{TontooRenderElements, WindowControlsElement, WindowTitlebarElement},
    render_cache::RenderCache,
    shell::window_controls::{self, TrafficLightAction, WindowControls},
    TontooCompositor,
};

/// Height of the server-side titlebar in logical pixels.
pub const BAR_HEIGHT: i32 = crate::config::TITLEBAR_HEIGHT;

/// Top strut reserved for the external Menubar.app. No SSD bar is drawn
/// above this line (maximized windows keep their content untouched).
pub const TOP_STRUT: f32 = 30.0;

/// App IDs that always get server-side decorations, no matter what they
/// request. These apps draw foreign (non-MacTahoe) headers themselves;
/// the compositor traffic-light bar replaces them. Matched case-insensitive
/// and exactly — extend when a new foreign-header app appears.
pub const FORCE_SSD_APP_IDS: &[&str] = &[
    "google-chrome",
    "chromium",
    "brave-browser",
    "microsoft-edge",
    "firefox",
    "org.mozilla.firefox",
    "code",
    "code-oss",
    "vscodium",
    "cursor",
];

/// Returns true when the app must use the compositor titlebar.
pub fn forces_ssd(app_id: &str) -> bool {
    let id = app_id.to_lowercase();
    FORCE_SSD_APP_IDS.iter().any(|known| id == *known)
}

/// Returns true when the window negotiated ServerSide decorations and the
/// client acked the configure (i.e. the mode is currently active).
pub fn is_ssd(window: &Window) -> bool {
    window
        .toplevel()
        .and_then(|t| {
            t.with_committed_state(|state| state.and_then(|s| s.decoration_mode))
        })
        == Some(Mode::ServerSide)
}

/// Returns true when the window is currently maximized.
pub fn is_maximized(window: &Window) -> bool {
    window
        .toplevel()
        .map(|t| {
            t.with_committed_state(|state| {
                state
                    .map(|s| s.states.contains(XdgState::Maximized))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Titlebar rectangle (output coordinates) for an SSD window, drawn directly
/// above the window content. Returns None when there is no room above the
/// window (maximized or top-edge windows).
pub fn bar_rect(geo: Rectangle<i32, Logical>) -> Option<Rectangle<f32, Logical>> {
    let h = BAR_HEIGHT as f32;
    let y = geo.loc.y as f32 - h;
    if y < TOP_STRUT {
        return None;
    }
    Some(Rectangle::new(
        Point::from((geo.loc.x as f32, y)),
        Size::from((geo.size.w as f32, h)),
    ))
}

/// Stable string key for per-window hover state.
pub fn window_key(window: &Window) -> String {
    window
        .toplevel()
        .map(|t| format!("{:?}", t.wl_surface().id()))
        .unwrap_or_default()
}

/// Hit-test a press position (output coordinates) against an SSD titlebar.
/// Returns the traffic-light action when a dot was hit, or None when the
/// press was on the bar background (start a move).
pub fn hit_test(bar: Rectangle<f32, Logical>, pos: Point<f64, Logical>) -> Option<TrafficLightAction> {
    let rel_x = pos.x as f32 - bar.loc.x;
    let rel_y = pos.y as f32 - bar.loc.y;
    WindowControls::new().hit_test(rel_x, rel_y)
}

/// Push titlebar background, traffic-light dots, hover symbols and the
/// centered window title for an SSD window. No-op when there is no room
/// for the bar (see `bar_rect`).
#[allow(clippy::too_many_arguments)]
pub fn push_ssd_elements(
    renderer: &mut GlesRenderer,
    cache: &mut RenderCache,
    controls: &mut HashMap<String, WindowControls>,
    focused: Option<&WlSurface>,
    scheme: ColorScheme,
    window: &Window,
    geo: Rectangle<i32, Logical>,
    title: Option<String>,
    out: &mut Vec<TontooRenderElements>,
) {
    let Some(bar) = bar_rect(geo) else {
        return;
    };
    let win_w = geo.size.w;
    let tb_h = BAR_HEIGHT;
    let win_x = bar.loc.x;
    let tb_y = bar.loc.y;

    // Titlebar background (per-width cache, scheme-aware macOS solid).
    let tb_key = (win_w, tb_h, scheme);
    if !cache.window_titlebars.contains_key(&tb_key) {
        if let Some(buf) = create_ssd_titlebar_texture(renderer, win_w, scheme) {
            cache.window_titlebars.insert(tb_key, buf);
        }
    }
    if let Some(ref buf) = cache.window_titlebars.get(&tb_key) {
        let elem = TextureRenderElement::from_texture_buffer(
            Point::from((win_x as f64, tb_y as f64)),
            buf,
            None,
            None,
            Some(Size::from((win_w, tb_h))),
            smithay::backend::renderer::element::Kind::Unspecified,
        );
        out.push(TontooRenderElements::WindowTitlebar(
            WindowTitlebarElement(elem),
        ));
    }

    let is_focused = window
        .toplevel()
        .map(|t| focused == Some(t.wl_surface()))
        .unwrap_or(false);

    let window_id = window_key(window);
    let is_hovered = controls
        .get(&window_id)
        .map(|c| c.hovered)
        .unwrap_or(false);

    let colors = if is_focused {
        [
            (
                "close".to_string(),
                window_controls::close_color(scheme),
            ),
            (
                "minimize".to_string(),
                window_controls::minimize_color(scheme),
            ),
            (
                "maximize".to_string(),
                window_controls::maximize_color(scheme),
            ),
        ]
    } else {
        [
            (
                "close_inactive".to_string(),
                window_controls::close_color_inactive(scheme),
            ),
            (
                "minimize_inactive".to_string(),
                window_controls::minimize_color_inactive(scheme),
            ),
            (
                "maximize_inactive".to_string(),
                window_controls::maximize_color_inactive(scheme),
            ),
        ]
    };

    let symbols = ['x', '-', '+'];
    let tc_scale: i32 = 2;
    let dot_size = window_controls::DOT_SIZE as i32;
    let dot_spacing = window_controls::DOT_SPACING;
    let left_pad = window_controls::LEFT_PADDING;
    let top_pad = window_controls::TOP_PADDING;

    for (i, (name, color)) in colors.iter().enumerate() {
        let dot_key = (
            name.clone(),
            dot_size * tc_scale,
            tc_scale,
            scheme,
        );
        if !cache.traffic_light_dots.contains_key(&dot_key) {
            let pixel_data =
                window_controls::create_traffic_light_dot(dot_size * tc_scale, *color);
            if let Ok(buf) = TextureBuffer::from_memory(
                renderer,
                &pixel_data,
                Fourcc::Abgr8888,
                (dot_size * tc_scale, dot_size * tc_scale),
                false,
                tc_scale,
                Transform::Normal,
                None,
            ) {
                cache.traffic_light_dots.insert(dot_key.clone(), buf);
            }
        }
        if let Some(ref buf) = cache.traffic_light_dots.get(&dot_key) {
            let dot_x = win_x + left_pad + i as f32 * (dot_size as f32 + dot_spacing);
            let dot_y = tb_y + top_pad;
            let elem = TextureRenderElement::from_texture_buffer(
                Point::from((dot_x as f64, dot_y as f64)),
                buf,
                None,
                None,
                Some(Size::from((dot_size, dot_size))),
                smithay::backend::renderer::element::Kind::Unspecified,
            );
            out.push(TontooRenderElements::WindowControls(
                WindowControlsElement(elem),
            ));
        }

        // Hover symbol (x - +) overlay.
        if is_hovered {
            let sym = symbols[i];
            let sym_name = format!("sym_{}_{}", name, sym);
            let sym_key = (
                sym_name.clone(),
                dot_size * tc_scale,
                tc_scale,
                scheme,
            );
            if !cache.traffic_light_dots.contains_key(&sym_key) {
                let pixel_data =
                    window_controls::create_traffic_light_symbol(dot_size * tc_scale, sym);
                if let Ok(buf) = TextureBuffer::from_memory(
                    renderer,
                    &pixel_data,
                    Fourcc::Abgr8888,
                    (dot_size * tc_scale, dot_size * tc_scale),
                    false,
                    tc_scale,
                    Transform::Normal,
                    None,
                ) {
                    cache.traffic_light_dots.insert(sym_key.clone(), buf);
                }
            }
            if let Some(ref buf) = cache.traffic_light_dots.get(&sym_key) {
                let dot_x = win_x + left_pad + i as f32 * (dot_size as f32 + dot_spacing);
                let dot_y = tb_y + top_pad;
                let elem = TextureRenderElement::from_texture_buffer(
                    Point::from((dot_x as f64, dot_y as f64)),
                    buf,
                    None,
                    None,
                    Some(Size::from((dot_size, dot_size))),
                    smithay::backend::renderer::element::Kind::Unspecified,
                );
                out.push(TontooRenderElements::WindowControls(
                    WindowControlsElement(elem),
                ));
            }
        }
    }

    // Centered window title.
    let title = title.unwrap_or_else(|| "TontooOS".to_string());
    let tb_text_color = match scheme {
        ColorScheme::Dark => [255u8, 255, 255, 255],
        ColorScheme::Light => [0u8, 0, 0, 255],
    };
    let font_size = 13.0;
    if let Some(text_buf) = render_text_texture(
        renderer,
        &title,
        font_size,
        tb_text_color,
        cache.font.as_ref(),
    ) {
        let text_w = (title.len() as f32 * font_size * 0.55) as f32;
        let center_x = win_x + (win_w as f32 - text_w) / 2.0;
        let text_y = tb_y + (BAR_HEIGHT as f32 - font_size) / 2.0;
        let elem = TextureRenderElement::from_texture_buffer(
            Point::from((center_x as f64, text_y as f64)),
            &text_buf,
            None,
            None,
            None,
            smithay::backend::renderer::element::Kind::Unspecified,
        );
        out.push(TontooRenderElements::WindowTitlebar(
            WindowTitlebarElement(elem),
        ));
    }
}

/// Titlebar background texture: solid macOS color, Dark `#1d1d1d`,
/// Light `#ececec`, rounded top corners.
fn create_ssd_titlebar_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    scheme: ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let tw = win_w as u32;
    let th = BAR_HEIGHT as u32;
    if tw == 0 {
        return None;
    }
    let mut data = vec![0u8; (tw * th * 4) as usize];

    let corner_r: f32 = 10.0;
    let (bg_r, bg_g, bg_b) = match scheme {
        ColorScheme::Dark => (0x1du8, 0x1du8, 0x1du8),
        ColorScheme::Light => (0xecu8, 0xecu8, 0xecu8),
    };

    for y in 0..th {
        for x in 0..tw {
            let i = ((y * tw + x) * 4) as usize;
            let alpha = if (x as f32) < corner_r && (y as f32) < corner_r {
                let dx = corner_r - x as f32;
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r {
                    0.0
                } else {
                    1.0 - (1.0 - dist / corner_r).powf(1.5)
                }
            } else if (x as f32) >= tw as f32 - corner_r && (y as f32) < corner_r {
                let dx = x as f32 - (tw as f32 - corner_r);
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r {
                    0.0
                } else {
                    1.0 - (1.0 - dist / corner_r).powf(1.5)
                }
            } else {
                1.0
            };
            let a = (alpha * 255.0) as u8;
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
        (win_w, BAR_HEIGHT),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

/// Rasterize single-line text with the cached UI font.
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

    struct Glyph {
        x: u32,
        width: u32,
        height: u32,
        bitmap: Vec<u8>,
    }
    let mut glyphs: Vec<Glyph> = Vec::new();

    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, px_size);
        let w = metrics.width as u32;
        let h = metrics.height as u32;
        if w > 0 && h > 0 {
            glyphs.push(Glyph {
                x: cursor_x,
                width: w,
                height: h,
                bitmap,
            });
            total_w = total_w.max(cursor_x + w);
        }
        cursor_x += metrics.advance_width as u32;
        if h > max_h {
            max_h = h;
        }
    }

    let total_h = if max_h > 0 { max_h } else { px_size as u32 };
    if total_w == 0 || total_h == 0 {
        return None;
    }

    let mut rgba = vec![0u8; (total_w * total_h * 4) as usize];
    for g in &glyphs {
        for row in 0..g.height {
            for col in 0..g.width {
                let alpha = g.bitmap[(row * g.width + col) as usize];
                if alpha == 0 {
                    continue;
                };
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
        renderer,
        &rgba,
        Fourcc::Abgr8888,
        (total_w as i32, total_h as i32),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

/// Surface id used to key per-window SSD state.
pub fn surface_id(window: &Window) -> Option<ObjectId> {
    window.toplevel().map(|t| t.wl_surface().id())
}

/// Close action: ask the client to close the window.
pub fn do_close(window: &Window) {
    if let Some(toplevel) = window.toplevel() {
        toplevel.send_close();
    }
}

/// Maximize toggle for SSD windows. Stores the pre-maximize geometry in
/// the compositor state so unmaximize restores it.
pub fn toggle_maximize(state: &mut TontooCompositor, window: &Window) {
    let Some(toplevel) = window.toplevel() else {
        return;
    };
    let Some(id) = surface_id(window) else {
        return;
    };

    if is_maximized(window) {
        if let Some(saved) = state.maximized_restore.remove(&id) {
            toplevel.with_pending_state(|s| {
                s.states.unset(XdgState::Maximized);
                s.size = Some(saved.size);
            });
            toplevel.send_pending_configure();
            state.space.map_element(window.clone(), saved.loc, true);
            state.space.raise_element(window, true);
        }
        return;
    }

    let Some(geo) = state.space.element_geometry(window) else {
        return;
    };
    let (work_loc, work_size) = work_area(state);
    state.maximized_restore.insert(id, geo);
    toplevel.with_pending_state(|s| {
        s.states.set(XdgState::Maximized);
        s.size = Some(work_size);
    });
    toplevel.send_pending_configure();
    state.space.map_element(window.clone(), work_loc, true);
    state.space.raise_element(window, true);
}

/// Output work area: full output minus the 30px Menubar.app top strut.
fn work_area(state: &TontooCompositor) -> (Point<i32, Logical>, Size<i32, Logical>) {
    let (loc, size) = state
        .space
        .outputs()
        .next()
        .and_then(|o| state.space.output_geometry(o))
        .map(|g| (g.loc, g.size))
        .unwrap_or((Point::from((0, 0)), Size::from((800, 600))));
    let strut = TOP_STRUT as i32;
    (
        Point::from((loc.x, loc.y + strut)),
        Size::from((size.w, (size.h - strut).max(100))),
    )
}

/// Minimize action: unmap the window and pin a temporary dock icon
/// (macOS behavior). Clicking the icon restores the window.
pub fn minimize_to_dock(state: &mut TontooCompositor, window: &Window, name: String) {
    state.space.unmap_elem(window);
    if !state
        .shell
        .dock
        .icons
        .iter()
        .any(|i| i.name == name)
    {
        state.shell.dock.add_icon(&name);
        state.minimized_icons.insert(name.clone());
    }
    for icon in state.shell.dock.icons.iter_mut() {
        if icon.name == name {
            icon.is_running = true;
        }
    }
    state.minimized_windows.push((name, window.clone()));
    if let Some(toplevel) = window.toplevel() {
        if state.focused_surface.as_ref() == Some(toplevel.wl_surface()) {
            state.focused_surface = None;
        }
    }
}
