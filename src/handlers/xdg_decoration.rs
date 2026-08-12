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
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
            state.size = Some(Size::from((DEFAULT_WIDTH, DEFAULT_HEIGHT)));
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        let effective = if mode == Mode::ClientSide {
            Mode::ClientSide
        } else {
            Mode::ServerSide
        };
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(effective);
            if effective == Mode::ServerSide {
                state.size = Some(Size::from((DEFAULT_WIDTH, DEFAULT_HEIGHT)));
            }
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = None;
        });
        toplevel.send_configure();
    }
}

smithay::delegate_xdg_decoration!(TontooCompositor);
