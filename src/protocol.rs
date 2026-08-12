pub mod tontoo_ui {
    pub use wayland_server;
    pub extern crate wayland_backend;

    pub mod __interfaces {
        wayland_scanner::generate_interfaces!("../protocol/tontoo_ui.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("../protocol/tontoo_ui.xml");
}
