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
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
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
                        Self::launch_terminal();
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

                    // ── 1. TontooUI surface clicks ──
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

                    // ── 2. Window clicks (single-click focus + activate) ──
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

                            if !is_same_window {
                                // First click on a new window: focus + raise it,
                                // then fall through so the click also reaches
                                // the app (single-click select + activate).
                                tracing::info!("Window click: focusing '{}' and passing through",
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
                            } else {
                                tracing::debug!("Window click: passing through to '{}'",
                                    window_app_name(&window).unwrap_or_default());
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

    /// Launch a terminal (Super+Enter shortcut). Apps are otherwise
    /// launched from the external Dock.app / LaunchPad.
    fn launch_terminal() {
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

        let terminals = ["foot", "alacritty", "kitty", "weston-terminal"];
        for t in &terminals {
            if let Ok(_child) = spawn_with_env(t) {
                tracing::info!("Launched terminal: {}", t);
                return;
            }
        }
        tracing::warn!("No terminal found! Tried: foot, alacritty, kitty, weston-terminal");
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
