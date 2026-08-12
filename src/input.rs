use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle, Size, SERIAL_COUNTER},
};

use crate::state::{get_app_id, get_window_title};
use crate::TontooCompositor;

const KEY_ENTER: u32 = 28;

// Function keys F1-F12 in evdev key codes (libinput)
const KEY_F1: u32 = 59;
const KEY_F12: u32 = 88;

impl TontooCompositor {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);

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
                self.update_traffic_light_hover(new_pos);
                self.update_tontoo_ui_hover(new_pos);
                let under = self.surface_under(new_pos);
                let pointer = self.seat.get_pointer().unwrap();

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: new_pos,
                        serial,
                        time: event.time_msec(),
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
                self.update_traffic_light_hover(pos);
                let under = self.surface_under(pos);
                let pointer = self.seat.get_pointer().unwrap();

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
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
                    if let Some(output) = self.space.outputs().next() {
                        if let Some(geo) = self.space.output_geometry(output) {
                            let sw = geo.size.w as f64;
                            let sh = geo.size.h as f64;
                            let icon_count = self.shell.dock.icons.len();
                            let rects = Self::dock_icon_rects(sw, sh, icon_count);
                            for (idx, rect) in rects.iter().enumerate() {
                                if rect.contains(pos_i) {
                                    let icon_name = self.shell.dock.icons[idx].name.clone();
                                    tracing::info!("Dock: '{}' clicked", icon_name);

                                    // Bounce the icon
                                    self.shell.dock.bounce_icon(&icon_name);
                                    self.shell.dock.set_active_app(&icon_name);

                                    // Update menubar app name
                                    self.shell.menubar.set_app_name(&icon_name);

                                    // Launch the app
                                    Self::launch_app_static(&icon_name);

                                    // Mark icon as running
                                    if let Some(icon) = self.shell.dock.icons.iter_mut().find(|i| i.name == icon_name) {
                                        icon.is_running = true;
                                    }

                                    return;
                                }
                            }
                        }
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

                    // ── 2. Window traffic light clicks + titlebar drag ──
                    if !pointer.is_grabbed() {
                        let mut needs_redraw = false;
                        let mut clicked_window: Option<(smithay::desktop::Window, i32, i32, f32, f32, Option<crate::shell::window_controls::TrafficLightAction>)> = None;

                        // First pass: find the clicked window (immutable borrow only)
                        for window in self.space.elements() {
                            if let Some(geo) = self.space.element_geometry(window) {
                                let win_x = geo.loc.x as f32;
                                let win_y = geo.loc.y as f32;
                                let win_w = geo.size.w as f32;
                                let tb_h = crate::config::TITLEBAR_HEIGHT as f32;
                                let tb_y = win_y - tb_h;

                                let rel_x = pos.x as f32 - win_x;
                                let rel_y_tb = pos.y as f32 - tb_y;

                                if rel_x >= 0.0 && rel_x < win_w && rel_y_tb >= 0.0 && rel_y_tb < tb_h {
                                    let window_id = format!("{}_{}", win_x as i32, win_y as i32);

                                    if !self.shell.window_controls.contains_key(&window_id) {
                                        self.shell.window_controls.insert(
                                            window_id.clone(),
                                            crate::shell::window_controls::WindowControls::new(),
                                        );
                                    }

                                    if let Some(action) = self.shell.window_controls[&window_id].hit_test(rel_x, rel_y_tb) {
                                        clicked_window = Some((window.clone(), win_x as i32, win_y as i32, rel_x, rel_y_tb, Some(action)));
                                    } else {
                                        clicked_window = Some((window.clone(), win_x as i32, win_y as i32, rel_x, rel_y_tb, None));
                                    }
                                    break;
                                }
                            }
                        }

                        // Second pass: handle the click (mutable borrow OK now)
                        if let Some((window, win_x, win_y, _rel_x, _rel_y_tb, action)) = clicked_window {
                            if let Some(action) = action {
                                // Traffic light clicked
                                tracing::info!("Traffic light clicked: {:?} on window at ({}, {})", action, win_x, win_y);
                                match action {
                                    crate::shell::window_controls::TrafficLightAction::Close => {
                                        window.toplevel().unwrap().send_close();
                                        needs_redraw = true;
                                    }
                                    crate::shell::window_controls::TrafficLightAction::Minimize => {
                                        needs_redraw = true;
                                    }
                                    crate::shell::window_controls::TrafficLightAction::Maximize => {
                                        let is_maximized = window.toplevel().unwrap().current_state().states.contains(xdg_toplevel::State::Maximized);
                                        window.toplevel().unwrap().with_pending_state(|state| {
                                            if is_maximized {
                                                state.states.unset(xdg_toplevel::State::Maximized);
                                                state.size = None;
                                            } else {
                                                state.states.set(xdg_toplevel::State::Maximized);
                                                state.size = None;
                                            }
                                        });
                                        window.toplevel().unwrap().send_pending_configure();
                                        needs_redraw = true;
                                    }
                                }
                            } else {
                                // Titlebar clicked (not on traffic lights) → start move grab
                                if let Some(geo) = self.space.element_geometry(&window) {
                                    let window_surface = window.toplevel().unwrap().wl_surface().clone();
                                    self.space.raise_element(&window, true);
                                    keyboard.set_focus(self, Some(window_surface.clone()), serial);
                                    self.focused_surface = Some(window_surface.clone());

                                    let start_data = smithay::input::pointer::GrabStartData {
                                        focus: None,
                                        location: pos,
                                        button,
                                    };

                                    if button == 0x110 {
                                        let grab = crate::grabs::MoveSurfaceGrab {
                                            start_data,
                                            window: window.clone(),
                                            initial_window_location: geo.loc,
                                        };
                                        pointer.set_grab(self, grab, serial, smithay::input::pointer::Focus::Clear);
                                        tracing::info!("Titlebar drag started on window at ({}, {})", win_x, win_y);
                                    }
                                }
                            }
                            return;
                        }

                        if needs_redraw {
                            self.request_redraw();
                        }
                    }

                    // ── 2. Window clicks (2-click behavior) ──
                    if !pointer.is_grabbed() {
                        if let Some((window, _loc)) = self
                            .space
                            .element_under(pointer.current_location())
                            .map(|(w, l)| (w.clone(), l))
                        {
                            let window_surface = window.toplevel().unwrap().wl_surface().clone();
                            let is_same_window = self.focused_surface.as_ref() == Some(&window_surface);

                            if is_same_window {
                                // Second click on same window: pass click through to app
                                tracing::debug!("Window second-click: passing through to '{}'",
                                    get_app_id(&window).or_else(|| get_window_title(&window)).unwrap_or_default());
                            } else {
                                // First click on new window: focus it, don't pass click through
                                tracing::info!("Window first-click: focusing '{}'",
                                    get_app_id(&window).or_else(|| get_window_title(&window)).unwrap_or_default());

                                self.space.raise_element(&window, true);
                                keyboard.set_focus(
                                    self,
                                    Some(window_surface.clone()),
                                    serial,
                                );
                                self.space.elements().for_each(|w| {
                                    w.toplevel().unwrap().send_pending_configure();
                                });

                                // Track focused surface
                                self.focused_surface = Some(window_surface.clone());

                                // Update dock active_app based on window app_id
                                let app_name = get_app_id(&window)
                                    .or_else(|| get_window_title(&window))
                                    .unwrap_or_else(|| "TontooOS".to_string());
                                self.shell.dock.set_active_app(&app_name);
                                self.shell.menubar.set_app_name(&app_name);

                                return;
                            }
                        } else {
                            // Clicked on empty space: deactivate everything
                            self.space.elements().for_each(|w| {
                                w.set_activated(false);
                                w.toplevel().unwrap().send_pending_configure();
                            });
                            keyboard.set_focus(self, Option::<WlSurface>::None, serial);

                            // Clear focused state
                            self.focused_surface = None;
                            self.shell.dock.clear_active_app();
                            self.shell.menubar.set_app_name("TontooOS");
                        }
                    }
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time_msec(),
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

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
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

    /// Update traffic light hover state based on pointer position.
    fn update_traffic_light_hover(&mut self, pos: Point<f64, Logical>) {
        for window in self.space.elements() {
            if let Some(geo) = self.space.element_geometry(window) {
                let win_x = geo.loc.x as f32;
                let win_y = geo.loc.y as f32;
                let win_w = geo.size.w as f32;
                let tb_h = crate::config::TITLEBAR_HEIGHT as f32;
                let tb_y = win_y - tb_h;

                let rel_x = pos.x as f32 - win_x;
                let rel_y = pos.y as f32 - tb_y;

                let window_id = format!("{}_{}", win_x as i32, win_y as i32);

                let is_in_area = rel_x >= 0.0 && rel_x < win_w
                    && rel_y >= 0.0 && rel_y < tb_h
                    && crate::shell::window_controls::WindowControls::is_in_area(rel_x, rel_y);

                if !self.shell.window_controls.contains_key(&window_id) {
                    self.shell.window_controls.insert(
                        window_id.clone(),
                        crate::shell::window_controls::WindowControls::new(),
                    );
                }

                if let Some(ctrl) = self.shell.window_controls.get_mut(&window_id) {
                    ctrl.hovered = is_in_area;
                }
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
