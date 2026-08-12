use crate::config::ColorScheme;
use crate::widget_renderer::DrawCommand;

#[derive(Debug, Clone)]
pub struct Topbar {
    pub visible: bool,
    pub height: f32,
}

impl Topbar {
    pub fn new() -> Self {
        Self {
            visible: true,
            height: 28.0,
        }
    }

    pub fn to_draw_commands(
        &self,
        screen_width: f32,
        color_scheme: ColorScheme,
    ) -> Vec<DrawCommand> {
        if !self.visible {
            return Vec::new();
        }

        let is_dark = color_scheme == ColorScheme::Dark;
        let milkiness = if is_dark { 0.0 } else { 0.3 };

        vec![DrawCommand::GlassPanel {
            x: 0.0,
            y: 0.0,
            width: screen_width,
            height: self.height,
            milkiness,
            alpha: 0.8,
            corner_radius: 0.0,
        }]
    }
}

impl Default for Topbar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_topbar_defaults() {
        let bar = Topbar::new();
        assert!(bar.visible);
        assert_eq!(bar.height, 28.0);
    }

    #[test]
    fn hidden_topbar_produces_no_commands() {
        let bar = Topbar {
            visible: false,
            ..Default::default()
        };
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);
        assert!(cmds.is_empty());
    }

    #[test]
    fn visible_topbar_produces_glass_panel() {
        let bar = Topbar::new();
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);
        assert_eq!(cmds.len(), 1);
        match &cmds[0] {
            DrawCommand::GlassPanel {
                x,
                y,
                width,
                height,
                alpha,
                corner_radius,
                ..
            } => {
                assert_eq!(*x, 0.0);
                assert_eq!(*y, 0.0);
                assert_eq!(*width, 1920.0);
                assert_eq!(*height, 28.0);
                assert_eq!(*alpha, 0.8);
                assert_eq!(*corner_radius, 0.0);
            }
            other => panic!("Expected GlassPanel, got {:?}", other),
        }
    }
}
