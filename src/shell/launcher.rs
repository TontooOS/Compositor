/// Application launcher overlay (e.g. macOS Spotlight / Launchpad style).
#[derive(Debug)]
pub struct Launcher {
    /// Whether the launcher overlay is currently visible.
    pub visible: bool,
}

impl Launcher {
    pub fn new() -> Self {
        Self { visible: false }
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

impl Default for Launcher {
    fn default() -> Self {
        Self::new()
    }
}
