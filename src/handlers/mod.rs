mod compositor;
mod layer_shell;
pub mod tontoo_ui;
pub mod xdg_decoration;
mod xdg_shell;

use crate::TontooCompositor;

use smithay::input::{dnd::DndGrabHandler, Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
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

        // Update tracked focused surface.
        // (The top menubar is the external Menubar.app system app and the
        // bottom dock is the external Dock.app system app; neither is
        // tracked inside the compositor.)
        self.focused_surface = focused.cloned();
    }
}

impl SelectionHandler for TontooCompositor {
    type SelectionUserData = ();
}

impl WaylandDndGrabHandler for TontooCompositor {}

impl DndGrabHandler for TontooCompositor {}

impl PointerConstraintsHandler for TontooCompositor {}

impl DataDeviceHandler for TontooCompositor {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl OutputHandler for TontooCompositor {}

smithay::delegate_dispatch2!(TontooCompositor);
