use smithay::{
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    utils::Size,
    wayland::shell::xdg::{
        decoration::XdgDecorationHandler,
        ToplevelSurface,
    },
};

use crate::TontooCompositor;

const DEFAULT_WIDTH: i32 = 800;
const DEFAULT_HEIGHT: i32 = 500;

impl XdgDecorationHandler for TontooCompositor {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        // Default to ClientSide (GTK/Qt apps always draw their own header),
        // unless the app is on the SSD enforcement list (Chrome, Firefox,
        // VSCode draw foreign headers — the compositor bar replaces them).
        let mode = self.ssd_mode_for(&toplevel);
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
            state.size = Some(Size::from((DEFAULT_WIDTH, DEFAULT_HEIGHT)));
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        // Enforce ServerSide for known foreign-header apps no matter what
        // they request; honor everyone else (KWin-style negotiation).
        let enforced = self.ssd_mode_for(&toplevel);
        let effective = if enforced == Mode::ServerSide {
            Mode::ServerSide
        } else {
            mode
        };
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(effective);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        // No client preference: fall back to the enforced/default mode.
        let mode = self.ssd_mode_for(&toplevel);
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });
        toplevel.send_configure();
    }
}

impl TontooCompositor {
    /// Decoration mode for a toplevel: `ServerSide` when its app ID is on
    /// the enforcement list, `ClientSide` otherwise. Unknown windows
    /// (not yet mapped) fall back to `ClientSide`.
    fn ssd_mode_for(&self, toplevel: &ToplevelSurface) -> Mode {
        let forced = self.space.elements().find_map(|window| {
            let candidate = window.toplevel()?;
            if candidate.wl_surface() != toplevel.wl_surface() {
                return None;
            }
            crate::state::get_app_id(window)
        });
        match forced {
            Some(id) if crate::shell::ssd::forces_ssd(&id) => Mode::ServerSide,
            _ => Mode::ClientSide,
        }
    }
}
