use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{
            AxisFrame, ButtonEvent, Focus, GrabStartData as PointerGrabStartData, MotionEvent,
        },
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{Logical, Point, Rectangle, Size, SERIAL_COUNTER},
};

use crate::grabs::MoveSurfaceGrab;
use crate::shell::window_controls::TrafficLightAction;
use crate::state::window_app_name;
use crate::TontooCompositor;

const KEY_ENTER: u32 = 28;

/// Left mouse button (evdev). Only this button triggers SSD titlebar
/// actions and window drags.
const BTN_LEFT: u32 = 0x110;

// Function keys F1-F12 in evdev key codes (libinput)
const KEY_F1: u32 = 59;
const KEY_F12: u32 = 88;

impl TontooCompositor {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = event.time();

                let keyboard = self.seat.get_keyboard().unwrap();

                let pressed = event.state() == smithay::backend::input::KeyState::Pressed;
                let key = event.key_code().raw();

                if key == KEY_ENTER {
                    let modifiers = keyboard.modifier_state();
                    tracing::debug!(
                        "Enter key: pressed={}, mods={{ ctrl={}, alt={}, shift={}, logo={} }}",
                        pressed,
                        modifiers.ctrl,
                        modifiers.alt,
                        modifiers.shift,
                        modifiers.logo
                    );
                    if pressed && modifiers.logo {
                        tracing::info!("Super+Enter pressed, launching terminal...");
                        Self::launch_app_static("Terminal");
                        return;
                    }
                }

                // Handle Ctrl+Alt+F* for VT switching
                if pressed && key >= KEY_F1 && key <= KEY_F12 {
                    let modifiers = keyboard.modifier_state();
                    if modifiers.ctrl && modifiers.alt {
                        let vt = (key - KEY_F1 + 1) as i32; // F1=VT1, F2=VT2, ...
                        tracing::info!(
                            "Ctrl+Alt+F{} pressed, switching to VT {}",
                            key - KEY_F1 + 1,
                            vt
                        );
                        #[cfg(feature = "udev")]
                        {
                            use smithay::backend::session::Session;
                            if let Some(ref mut udev_data) = self.udev_data {
                                match udev_data.session.change_vt(vt) {
                                    Ok(_) => return,
                                    Err(e) => {
                                        tracing::error!("Failed to switch to VT {}: {}", vt, e)
                                    }
                                }
                            } else {
                                tracing::warn!(
                                    "Ctrl+Alt+F{} pressed but no udev session available",
                                    vt
                                );
                            }
                        }
                        #[cfg(not(feature = "udev"))]
                        {
                            tracing::warn!(
                                "Ctrl+Alt+F{} pressed but compositor built without udev",
                                vt
                            );
                        }
                    }
                }

                keyboard.input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
                // Keyboard needs immediate visual feedback - don't wait for 16ms timer
                self.pending_redraw = true;
                let _ = self.loop_signal.wakeup();
            }
            InputEvent::PointerMotion { event, .. } => {
                let delta = event.delta();
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().unwrap();
                let current = pointer.current_location();
                let new_pos = Point::from((current.x + delta.x, current.y + delta.y));
                drop(pointer);
                self.cursor.update_speed(new_pos);
                self.update_dock_hover(new_pos);
                self.update_tontoo_ui_hover(new_pos);
                self.update_ssd_hover(new_pos);
                let under = self.surface_under(new_pos);
                let pointer = self.seat.get_pointer().unwrap();

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: new_pos,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let output = match self.space.outputs().next() {
                    Some(o) => o,
                    None => return,
                };

                let output_geo = match self.space.output_geometry(output) {
                    Some(g) => g,
                    None => return,
                };

                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
                let serial = SERIAL_COUNTER.next_serial();
                self.cursor.update_speed(pos);
                self.update_dock_hover(pos);
                self.update_ssd_hover(pos);
                let under = self.surface_under(pos);
                let pointer = self.seat.get_pointer().unwrap();

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let button_state = event.state();

                if ButtonState::Pressed == button_state {
                    let pos = pointer.current_location();
                    let pos_i = Point::from((pos.x as i32, pos.y as i32));

                    // ── 1. Dock icon clicks (always processed first) ──
                    // Hit test first with short-lived borrows, then act.
                    let dock_hit: Option<String> = self
                        .space
                        .outputs()
                        .next()
                        .and_then(|o| self.space.output_geometry(o))
                        .map(|geo| {
                            let sw = geo.size.w as f64;
                            let sh = geo.size.h as f64;
                            let icon_count = self.shell.dock.icons.len();
                            let rects = Self::dock_icon_rects(sw, sh, icon_count);
                            rects
                                .iter()
                                .enumerate()
                                .find(|(_, rect)| rect.contains(pos_i))
                                .map(|(idx, _)| self.shell.dock.icons[idx].name.clone())
                        })
                        .flatten();
                    if let Some(icon_name) = dock_hit {
                        // Restore a minimized window instead of launching
                        // when a matching minimized window exists.
                        if let Some(min_idx) = self
                            .minimized_windows
                            .iter()
                            .position(|(n, _)| *n == icon_name)
                        {
                            let (name, window) = self.minimized_windows.remove(min_idx);
                            let alive = window
                                .toplevel()
                                .map(|t| t.wl_surface().is_alive())
                                .unwrap_or(false);
                            if alive {
                                tracing::info!("Dock: restoring minimized '{}'", name);
                                let size = window.geometry().size;
                                let loc = Self::center_on_output(&self.space, size);
                                self.space.map_element(window.clone(), loc, true);
                                self.space.raise_element(&window, true);
                                let window_surface =
                                    window.toplevel().unwrap().wl_surface().clone();
                                keyboard.set_focus(
                                    self,
                                    Some(window_surface.clone()),
                                    serial,
                                );
                                window.toplevel().unwrap().send_pending_configure();
                                self.focused_surface = Some(window_surface);
                                self.shell.dock.set_active_app(&name);
                                if self.minimized_icons.remove(&name) {
                                    self.shell.dock.remove_icon(&name);
                                }
                                self.pending_redraw = true;
                                let _ = self.loop_signal.wakeup();
                                return;
                            }
                            // Stale entry (client exited): drop the temp
                            // icon and fall through to launching.
                            if self.minimized_icons.remove(&name) {
                                self.shell.dock.remove_icon(&name);
                            }
                        }

                        tracing::info!("Dock: '{}' clicked", icon_name);

                        // Bounce the icon
                        self.shell.dock.bounce_icon(&icon_name);
                        self.shell.dock.set_active_app(&icon_name);

                        // Launch the app
                        Self::launch_app_static(&icon_name);

                        // Mark icon as running
                        if let Some(icon) = self.shell.dock.icons.iter_mut().find(|i| i.name == icon_name) {
                            icon.is_running = true;
                        }

                        return;
                    }

                    // ── 2. TontooUI surface clicks ──
                    if !pointer.is_grabbed() {
                        let mut handled = false;
                        if let Some(output) = self.space.outputs().next() {
                            if let Some(geo) = self.space.output_geometry(output) {
                                let sw = geo.size.w as f32;
                                let sh = geo.size.h as f32;
                                for surf in self.tontoo_ui.surfaces() {
                                    if surf.parsed_tree.is_empty() { continue; }
                                    let surf_w = surf.width as f32;
                                    let surf_h = surf.height as f32;
                                    let sx = (sw - surf_w) / 2.0;
                                    let sy = (sh - surf_h) / 2.0;

                                    // Is click inside this surface?
                                    if pos.x >= sx as f64 && pos.x <= (sx + surf_w) as f64
                                        && pos.y >= sy as f64 && pos.y <= (sy + surf_h) as f64
                                    {
                                        // Hit-test widgets
                                        let local_x = pos.x as f32 - sx;
                                        let local_y = pos.y as f32 - sy;
                                        if let Some(node_id) = crate::widget_tree::hit_test(&surf.parsed_tree, local_x, local_y) {
                                            tracing::info!("tontoo_ui: click on node_id={}", node_id);
                                            surf.send_widget_clicked(node_id as u32);
                                        }
                                        handled = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if handled {
                            return;
                        }
                    }

                    // ── SSD titlebar clicks (traffic lights + drag) ──
                    // Only for windows with negotiated server-side decorations
                    // (Chrome/VSCode with system title bar). Topmost first so
                    // overlapping windows resolve correctly.
                    if !pointer.is_grabbed() && button == BTN_LEFT {
                        let hit = self.space.elements().rev().find_map(|window| {
                            let geo = self.space.element_geometry(window)?;
                            if !crate::shell::ssd::is_ssd(window) {
                                return None;
                            }
                            if crate::shell::ssd::is_maximized(window) {
                                return None;
                            }
                            let bar = crate::shell::ssd::bar_rect(geo)?;
                            let in_bar = pos.x >= bar.loc.x as f64
                                && pos.x <= (bar.loc.x + bar.size.w) as f64
                                && pos.y >= bar.loc.y as f64
                                && pos.y <= (bar.loc.y + bar.size.h) as f64;
                            if !in_bar {
                                return None;
                            }
                            Some((window.clone(), geo, bar))
                        });

                        if let Some((window, geo, bar)) = hit {
                            // Focus + raise like a normal window click.
                            let window_surface =
                                window.toplevel().unwrap().wl_surface().clone();
                            self.space.raise_element(&window, true);
                            keyboard.set_focus(self, Some(window_surface.clone()), serial);
                            self.focused_surface = Some(window_surface.clone());
                            let app_name = window_app_name(&window)
                                .unwrap_or_else(|| "TontooOS".to_string());
                                                        self.shell.dock.set_active_app(&app_name);

                            match crate::shell::ssd::hit_test(bar, pos) {
                                Some(action) => {
                                    match action {
                                        TrafficLightAction::Close => {
                                            tracing::info!("SSD: close '{}'", app_name);
                                            crate::shell::ssd::do_close(&window);
                                        }
                                        TrafficLightAction::Minimize => {
                                            tracing::info!("SSD: minimize '{}'", app_name);
                                            keyboard.set_focus(
                                                self,
                                                Option::<WlSurface>::None,
                                                serial,
                                            );
                                            crate::shell::ssd::minimize_to_dock(
                                                self,
                                                &window,
                                                app_name,
                                            );
                                        }
                                        TrafficLightAction::Maximize => {
                                            tracing::info!("SSD: maximize toggle '{}'", app_name);
                                            crate::shell::ssd::toggle_maximize(self, &window);
                                        }
                                    }
                                    self.pending_redraw = true;
                                    let _ = self.loop_signal.wakeup();
                                    return;
                                }
                                None => {
                                    // Bar background: start a move drag.
                                    let surf_loc = Point::from((
                                        pos.x - geo.loc.x as f64,
                                        pos.y - geo.loc.y as f64,
                                    ));
                                    let start_data = PointerGrabStartData {
                                        focus: Some((window_surface, surf_loc)),
                                        button,
                                        location: pos,
                                    };
                                    let grab = MoveSurfaceGrab {
                                        start_data,
                                        window: window.clone(),
                                        initial_window_location: geo.loc,
                                    };
                                    pointer.set_grab(self, grab, serial, Focus::Clear);
                                    return;
                                }
                            }
                        }
                    }

                    // ── 2. Window clicks (2-click focus behavior) ──
                    if !pointer.is_grabbed() {
                        if let Some((window, _loc)) = self
                            .space
                            .element_under(pointer.current_location())
                            .map(|(w, l)| (w.clone(), l))
                        {
                            // X11 windows have no xdg toplevel; use their wl
                            // surface instead (udev backend with XWayland).
                            #[cfg(feature = "udev")]
                            let window_surface =
                                crate::xwayland::window_wl_surface(&window);
                            #[cfg(not(feature = "udev"))]
                            let window_surface: Option<WlSurface> = window
                                .toplevel()
                                .map(|t| t.wl_surface().clone());
                            let Some(window_surface) = window_surface else {
                                // No focusable surface: forward the click.
                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
                return;
            };
                            let is_same_window = self.focused_surface.as_ref() == Some(&window_surface);

                            if is_same_window {
                                // Second click on same window: pass click through to app
                                tracing::debug!("Window second-click: passing through to '{}'",
                                    window_app_name(&window).unwrap_or_default());
                            } else {
                                // First click on new window: focus it, don't pass click through
                                tracing::info!("Window first-click: focusing '{}'",
                                    window_app_name(&window).unwrap_or_default());

                                self.space.raise_element(&window, true);
                                keyboard.set_focus(
                                    self,
                                    Some(window_surface.clone()),
                                    serial,
                                );
                                self.space.elements().for_each(|w| {
                                    if let Some(toplevel) = w.toplevel() {
                                        toplevel.send_pending_configure();
                                    }
                                });

                                // Track focused surface
                                self.focused_surface = Some(window_surface.clone());

                                // Update dock active_app based on window app_id
                                let app_name = window_app_name(&window)
                                    .unwrap_or_else(|| "TontooOS".to_string());
                                self.shell.dock.set_active_app(&app_name);

                                return;
                            }
                        } else {
                            // Clicked on empty space: deactivate everything
                            self.space.elements().for_each(|w| {
                                w.set_activated(false);
                                if let Some(toplevel) = w.toplevel() {
                                    toplevel.send_pending_configure();
                                }
                            });
                            keyboard.set_focus(self, Option::<WlSurface>::None, serial);

                            // Clear focused state
                            self.focused_surface = None;
                            self.shell.dock.clear_active_app();
                        }
                    }
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.
                });
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    /// Compute dock icon hit rectangles for all icons given screen dimensions.
    fn dock_icon_rects(screen_w: f64, screen_h: f64, icon_count: usize) -> Vec<Rectangle<i32, Logical>> {
        const DOCK_H: f64 = 78.0;
        const BOTTOM_MARGIN: f64 = 15.0;
        const ICON_SIZE: f64 = 48.0;
        const ICON_GAP: f64 = 12.0;

        if icon_count == 0 {
            return Vec::new();
        }

        let count = icon_count;
        let total_icons_w = ICON_SIZE * count as f64 + ICON_GAP * (count as f64 - 1.0).max(0.0);
        let dock_w = (total_icons_w + 32.0).min(screen_w - 40.0);
        let dx = (screen_w - dock_w) / 2.0;
        let dy = screen_h - DOCK_H - BOTTOM_MARGIN;
        let icon_y = dy + (DOCK_H - ICON_SIZE) / 2.0;
        let icon_start_x = dx + (dock_w - total_icons_w) / 2.0;

        let mut rects = Vec::with_capacity(count);
        for i in 0..count {
            let x = icon_start_x + i as f64 * (ICON_SIZE + ICON_GAP);
            rects.push(Rectangle::<i32, Logical>::new(
                Point::from((x as i32, icon_y as i32)),
                Size::from((ICON_SIZE as i32, ICON_SIZE as i32)),
            ));
        }
        rects
    }

    /// Update dock hover state based on pointer position.
    fn update_dock_hover(&mut self, pos: Point<f64, Logical>) {
        if let Some(output) = self.space.outputs().next() {
            if let Some(geo) = self.space.output_geometry(output) {
                let sw = geo.size.w as f64;
                let sh = geo.size.h as f64;
                let icon_count = self.shell.dock.icons.len();
                let rects = Self::dock_icon_rects(sw, sh, icon_count);
                let pos_i = Point::from((pos.x as i32, pos.y as i32));
                let mut hovered = None;
                for (idx, rect) in rects.iter().enumerate() {
                    if rect.contains(pos_i) {
                        hovered = Some(idx);
                        break;
                    }
                }
                self.shell.dock.set_hover(hovered);
            }
        }
    }

    /// Launch an application by dock icon name.
    fn launch_app_static(name: &str) {
        // Ensure child processes can connect to our Wayland display
        let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/run/liveuser".to_string());

        let spawn_with_env = |cmd: &str| -> Result<std::process::Child, std::io::Error> {
            std::process::Command::new(cmd)
                .env("WAYLAND_DISPLAY", &wayland_display)
                .env("XDG_RUNTIME_DIR", &xdg_runtime)
                .env("XDG_SESSION_TYPE", "wayland")
                .env("MOZ_ENABLE_WAYLAND", "1")
                .spawn()
        };

        match name {
            "Finder" => {
                let file_managers = ["nautilus", "dolphin", "thunar", "pcmanfm", "nemo"];
                for fm in &file_managers {
                    if let Ok(_child) = spawn_with_env(fm) {
                        tracing::info!("Launched file manager: {}", fm);
                        return;
                    }
                }
                tracing::warn!("No file manager found!");
            }
            "Terminal" => {
                let terminals = ["foot", "alacritty", "kitty", "weston-terminal"];
                for t in &terminals {
                    if let Ok(_child) = spawn_with_env(t) {
                        tracing::info!("Launched terminal: {}", t);
                        return;
                    }
                }
                tracing::warn!("No terminal found! Tried: foot, alacritty, kitty, weston-terminal");
            }
            "Settings" => {
                let settings = ["gnome-control-center", "systemsettings", "xfce4-settings-editor"];
                for s in &settings {
                    if let Ok(_child) = spawn_with_env(s) {
                        tracing::info!("Launched settings: {}", s);
                        return;
                    }
                }
                tracing::warn!("No settings app found!");
            }
            "Notes" => {
                tracing::info!("Notes app: placeholder (coming soon)");
            }
            "Podcasts" => {
                tracing::info!("Podcasts app: placeholder (coming soon)");
            }
            other => {
                if let Ok(_child) = spawn_with_env(other) {
                    tracing::info!("Launched app: {}", other);
                    return;
                }
                tracing::warn!("Unknown app: {}", other);
            }
        }
    }

    /// Track pointer hover over SSD titlebars for traffic-light symbols.
    fn update_ssd_hover(&mut self, pos: Point<f64, Logical>) {
        let mut changed = false;
        for window in self.space.elements() {
            if !crate::shell::ssd::is_ssd(window) {
                continue;
            }
            let Some(geo) = self.space.element_geometry(window) else {
                continue;
            };
            let Some(bar) = crate::shell::ssd::bar_rect(geo) else {
                continue;
            };
            let inside = pos.x >= bar.loc.x as f64
                && pos.x <= (bar.loc.x + bar.size.w) as f64
                && pos.y >= bar.loc.y as f64
                && pos.y <= (bar.loc.y + bar.size.h) as f64;
            let key = crate::shell::ssd::window_key(window);
            let entry = self.shell.window_controls.entry(key).or_default();
            if entry.hovered != inside {
                entry.hovered = inside;
                changed = true;
            }
        }
        if changed {
            self.pending_redraw = true;
            let _ = self.loop_signal.wakeup();
        }
    }

    /// Center a window of the given size on the primary output.
    fn center_on_output(
        space: &smithay::desktop::Space<smithay::desktop::Window>,
        size: Size<i32, Logical>,
    ) -> Point<i32, Logical> {
        let (out_loc, out_size) = space
            .outputs()
            .next()
            .and_then(|o| space.output_geometry(o))
            .map(|g| (g.loc, g.size))
            .unwrap_or((Point::from((0, 0)), Size::from((800, 600))));
        Point::from((
            out_loc.x + (out_size.w - size.w).max(0) / 2,
            out_loc.y + (out_size.h - size.h).max(0) / 2,
        ))
    }

    /// Track pointer hover over tontoo_ui surfaces and send widget_hovered events.
    fn update_tontoo_ui_hover(&mut self, pos: Point<f64, Logical>) {
        if let Some(output) = self.space.outputs().next() {
            if let Some(geo) = self.space.output_geometry(output) {
                let sw = geo.size.w as f32;
                let sh = geo.size.h as f32;
                for surf in self.tontoo_ui.surfaces_mut() {
                    if surf.parsed_tree.is_empty() { continue; }
                    let surf_w = surf.width as f32;
                    let surf_h = surf.height as f32;
                    let sx = (sw - surf_w) / 2.0;
                    let sy = (sh - surf_h) / 2.0;

                    let inside = pos.x >= sx as f64 && pos.x <= (sx + surf_w) as f64
                        && pos.y >= sy as f64 && pos.y <= (sy + surf_h) as f64;

                    let hovered = if inside {
                        let local_x = pos.x as f32 - sx;
                        let local_y = pos.y as f32 - sy;
                        crate::widget_tree::hit_test(&surf.parsed_tree, local_x, local_y)
                    } else {
                        None
                    };

                    if hovered != surf.last_hovered_node {
                        let node_id = hovered.unwrap_or(0) as u32;
                        surf.send_widget_hovered(node_id);
                        surf.last_hovered_node = hovered;
                    }
                }
            }
        }
    }
}
