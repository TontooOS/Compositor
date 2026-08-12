mod compositor;
mod layer_shell;
pub mod tontoo_ui;
pub mod xdg_decoration;
mod xdg_shell;

use crate::TontooCompositor;
use crate::state::{get_app_id, get_window_title};

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;

impl SeatHandler for TontooCompositor {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<TontooCompositor> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        tracing::debug!("SeatHandler::cursor_image called");
        self.cursor.handle_cursor_image(image);
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        self.cursor.reset_visibility();
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);

        // Update tracked focused surface
        self.focused_surface = focused.cloned();

        // Update dock active_app and menubar based on focused window
        if let Some(wl_surface) = focused {
            // Find the window with this surface
            if let Some(window) = self.space.elements().find(|w| {
                w.toplevel().unwrap().wl_surface() == wl_surface
            }).cloned() {
                let app_name = get_app_id(&window)
                    .or_else(|| get_window_title(&window))
                    .unwrap_or_else(|| "TontooOS".to_string());
                self.shell.dock.set_active_app(&app_name);
                self.shell.menubar.set_app_name(&app_name);
            }
        } else {
            self.shell.dock.clear_active_app();
            self.shell.menubar.set_app_name("TontooOS");
        }
    }
}

impl SelectionHandler for TontooCompositor {
    type SelectionUserData = ();
}

impl ClientDndGrabHandler for TontooCompositor {}
impl ServerDndGrabHandler for TontooCompositor {}

impl DataDeviceHandler for TontooCompositor {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl OutputHandler for TontooCompositor {}

smithay::delegate_compositor!(TontooCompositor);
smithay::delegate_shm!(TontooCompositor);
smithay::delegate_output!(TontooCompositor);
smithay::delegate_seat!(TontooCompositor);
smithay::delegate_xdg_shell!(TontooCompositor);
smithay::delegate_data_device!(TontooCompositor);
