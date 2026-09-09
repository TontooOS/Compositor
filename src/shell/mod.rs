pub mod dock;
pub mod launcher;
pub mod ssd;
pub mod topbar;
pub mod window_controls;

pub use dock::Dock;
pub use launcher::Launcher;
pub use topbar::Topbar;
pub use window_controls::WindowControls;

pub struct ShellState {
    pub dock: Dock,
    pub launcher: Launcher,
    pub topbar: Topbar,
    /// Per-window traffic light state. Key is a string identifier for the window.
    pub window_controls: std::collections::HashMap<String, WindowControls>,
}

impl ShellState {
    pub fn new() -> Self {
        let mut dock = Dock::new();
        dock.add_icon("Finder");
        dock.add_icon("Terminal");
        dock.add_icon("Settings");
        dock.add_icon("Notes");
        dock.add_icon("Podcasts");

        Self {
            dock,
            launcher: Launcher::new(),
            topbar: Topbar::new(),
            window_controls: std::collections::HashMap::new(),
        }
    }

    pub fn launcher_visible(&self) -> bool {
        self.launcher.visible
    }

    pub fn toggle_launcher(&mut self) {
        if self.launcher.visible {
            self.launcher.hide();
        } else {
            self.launcher.show();
        }
    }

    pub fn dock_height(&self) -> f32 {
        self.dock.height
    }

    /// Reserved top strut for the external Menubar.app (system app, not
    /// rendered by the compositor). Windows are placed below this area.
    pub fn menubar_height(&self) -> f32 {
        30.0
    }
}
