//! macOS-style top menu bar for TontooOS.
//!
//! Renders a translucent glass panel spanning the full screen width at the
//! top of the display.  The **left** side shows the TontooOS logo icon
//! followed by the active application name; the **right** side shows
//! system tray icons for Wi-Fi, battery, and a clock.
//!
//! All rendering is expressed as [`DrawCommand`]s — the compositor's
//! [`WidgetRenderer`] takes care of rasterising them.

use crate::config::ColorScheme;
use crate::widget_renderer::{Color, DrawCommand};

// ═══════════════════════════════════════════════════════════════
// MenubarItem — a single clickable element inside the bar
// ═══════════════════════════════════════════════════════════════

/// A single item in the menu bar (either a left-side OS / app menu
/// entry or a right-side system-tray icon).
#[derive(Debug, Clone)]
pub struct MenubarItem {
    pub label: String,
    pub icon: Option<String>,
}

impl MenubarItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

// ═══════════════════════════════════════════════════════════════
// Menubar — the full top-bar component
// ═══════════════════════════════════════════════════════════════

/// macOS-style top menu bar.
#[derive(Debug, Clone)]
pub struct Menubar {
    /// Bar height in logical pixels.
    pub height: f32,
    /// Whether the bar is currently rendered.
    pub visible: bool,
    /// Whether the TontooOS dropdown menu is open.
    pub os_menu_active: bool,
    /// Name of the currently focused application.
    pub app_name: String,
    /// Show the clock on the right side.
    pub show_clock: bool,
    /// Show the Wi-Fi icon on the right side.
    pub show_wifi: bool,
    /// Show the battery icon on the right side.
    pub show_battery: bool,
}

impl Menubar {
    /// Create a new menubar with sensible defaults.
    pub fn new() -> Self {
        Self {
            height: 28.0,
            visible: true,
            os_menu_active: false,
            app_name: String::from("TontooOS"),
            show_clock: true,
            show_wifi: true,
            show_battery: true,
        }
    }

    /// Update the application name shown next to the OS logo.
    pub fn set_app_name(&mut self, name: &str) {
        self.app_name = name.to_string();
    }

    /// Toggle the TontooOS system menu open/closed.
    pub fn toggle_os_menu(&mut self) {
        self.os_menu_active = !self.os_menu_active;
    }

    // ── Rendering ───────────────────────────────────────────

    /// Flatten the menubar into draw commands for the given `screen_width`.
    ///
    /// The colour palette adapts to `color_scheme`:
    /// * **Dark** — near-transparent glass with white text.
    /// * **Light** — milky glass with dark text.
    pub fn to_draw_commands(
        &self,
        screen_width: f32,
        color_scheme: ColorScheme,
    ) -> Vec<DrawCommand> {
        if !self.visible {
            return Vec::new();
        }

        let is_dark = color_scheme == ColorScheme::Dark;

        // Colours ─────────────────────────────────────────────
        let text_color = if is_dark {
            Color::new(0.92, 0.92, 0.94, 1.0) // near-white
        } else {
            Color::new(0.11, 0.11, 0.12, 1.0) // near-black
        };

        let icon_color = if is_dark {
            Color::new(0.82, 0.82, 0.84, 1.0)
        } else {
            Color::new(0.22, 0.22, 0.24, 1.0)
        };

        let clock_color = if is_dark {
            Color::new(0.88, 0.88, 0.90, 1.0)
        } else {
            Color::new(0.15, 0.15, 0.17, 1.0)
        };

        let highlight_bg = if is_dark {
            Color::new(1.0, 1.0, 1.0, 0.12)
        } else {
            Color::new(0.0, 0.0, 0.0, 0.08)
        };

        let font_size = 13.0;
        let vertical_pad = 6.0;
        let mut cmds: Vec<DrawCommand> = Vec::with_capacity(12);

        // ── 1. Glass panel background ───────────────────────
        let milkiness = if is_dark { 0.0 } else { 0.3 };
        cmds.push(DrawCommand::GlassPanel {
            x: 0.0,
            y: 0.0,
            width: screen_width,
            height: self.height,
            milkiness,
            alpha: 0.8,
            corner_radius: 0.0,
        });

        let text_y = vertical_pad;

        // ── 2. Left side: OS logo + app name ────────────────
        let mut left_x = 12.0;

        // TontooOS logo (small AppIcon rendered as a rounded rect
        // placeholder — 16×16, centred vertically).
        let logo_size = 16.0;
        let logo_y = (self.height - logo_size) / 2.0;
        let logo_bg = Color::new(0.42, 0.56, 0.96, 1.0); // brand blue
        cmds.push(DrawCommand::Rect {
            x: left_x,
            y: logo_y,
            width: logo_size,
            height: logo_size,
            color: logo_bg,
            corner_radius: 4.0,
        });
        cmds.push(DrawCommand::Text {
            content: "T".to_string(),
            x: left_x + 3.0,
            y: text_y + 1.0,
            font_size: 11.0,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
            max_width: None,
        });
        left_x += logo_size + 8.0;

        // App name (bold label)
        cmds.push(DrawCommand::Text {
            content: self.app_name.clone(),
            x: left_x,
            y: text_y,
            font_size,
            color: text_color,
            max_width: None,
        });
        left_x += self.estimate_text_width(&self.app_name, font_size) + 16.0;

        // OS menu button (highlighted when active)
        if self.os_menu_active {
            let btn_w = self.estimate_text_width("TontooOS", font_size) + 16.0;
            cmds.push(DrawCommand::Rect {
                x: left_x - 4.0,
                y: 0.0,
                width: btn_w,
                height: self.height,
                color: highlight_bg,
                corner_radius: 4.0,
            });
        }
        cmds.push(DrawCommand::Text {
            content: "TontooOS".to_string(),
            x: left_x,
            y: text_y,
            font_size,
            color: if self.os_menu_active {
                text_color
            } else {
                icon_color
            },
            max_width: None,
        });

        // ── 3. Right side: system tray icons ────────────────
        let mut right_x = screen_width - 12.0;

        // Clock (rightmost)
        if self.show_clock {
            let clock_text = self.current_time_string();
            let clock_w = self.estimate_text_width(&clock_text, font_size);
            right_x -= clock_w;
            cmds.push(DrawCommand::Text {
                content: clock_text,
                x: right_x,
                y: text_y,
                font_size,
                color: clock_color,
                max_width: None,
            });
            right_x -= 16.0;
        }

        // Battery icon
        if self.show_battery {
            let bat_icon = "\u{25A0}"; // ■ placeholder
            let bat_w = self.estimate_text_width(bat_icon, font_size);
            right_x -= bat_w;
            cmds.push(DrawCommand::Text {
                content: bat_icon.to_string(),
                x: right_x,
                y: text_y,
                font_size,
                color: icon_color,
                max_width: None,
            });
            right_x -= 12.0;
        }

        // Wi-Fi icon
        if self.show_wifi {
            let wifi_icon = "\u{25CE}"; // ◎ placeholder
            let wifi_w = self.estimate_text_width(wifi_icon, font_size);
            right_x -= wifi_w;
            cmds.push(DrawCommand::Text {
                content: wifi_icon.to_string(),
                x: right_x,
                y: text_y,
                font_size,
                color: icon_color,
                max_width: None,
            });
        }

        // ── 4. OS dropdown menu (when active) ───────────────
        if self.os_menu_active {
            let menu_width = 220.0;
            let menu_height = 180.0;
            let menu_x = 8.0;
            let menu_y = self.height;

            // Menu background (slightly lighter/darker than bar)
            let menu_bg = if is_dark {
                Color::new(0.16, 0.16, 0.18, 0.95)
            } else {
                Color::new(0.98, 0.98, 0.98, 0.95)
            };
            cmds.push(DrawCommand::Rect {
                x: menu_x,
                y: menu_y,
                width: menu_width,
                height: menu_height,
                color: menu_bg,
                corner_radius: 6.0,
            });

            // Menu items
            let items = [
                "About TontooOS",
                "", // separator
                "System Preferences...",
                "App Store...",
                "", // separator
                "Force Quit...",
                "", // separator
                "Sleep",
                "Restart...",
                "Shut Down...",
                "", // separator
                "Lock Screen",
            ];

            let item_height = 22.0;
            let mut item_y = menu_y + 6.0;

            for item in &items {
                if item.is_empty() {
                    // Thin separator line
                    cmds.push(DrawCommand::Rect {
                        x: menu_x + 8.0,
                        y: item_y + (item_height - 1.0) / 2.0,
                        width: menu_width - 16.0,
                        height: 1.0,
                        color: if is_dark {
                            Color::new(1.0, 1.0, 1.0, 0.1)
                        } else {
                            Color::new(0.0, 0.0, 0.0, 0.1)
                        },
                        corner_radius: 0.0,
                    });
                    item_y += item_height;
                    continue;
                }

                let item_color = if *item == "Force Quit..." || *item == "Shut Down..." {
                    Color::new(0.94, 0.33, 0.31, 1.0) // red for destructive
                } else {
                    text_color
                };

                cmds.push(DrawCommand::Text {
                    content: (*item).to_string(),
                    x: menu_x + 14.0,
                    y: item_y + 2.0,
                    font_size: 13.0,
                    color: item_color,
                    max_width: Some(menu_width - 28.0),
                });
                item_y += item_height;
            }
        }

        cmds
    }

    // ── Helpers ─────────────────────────────────────────────

    /// Very rough monospace-style text width estimate.
    ///
    /// Each glyph ≈ `font_size × 0.55` wide.  Good enough for
    /// positioning text without a full shaping run.
    fn estimate_text_width(&self, text: &str, font_size: f32) -> f32 {
        text.len() as f32 * font_size * 0.55
    }

    /// Return a HH:MM string for the current wall-clock time.
    fn current_time_string(&self) -> String {
        use std::time::SystemTime;

        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;

        format!("{:02}:{:02}", hours, minutes)
    }
}

impl Default for Menubar {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_menubar_has_defaults() {
        let bar = Menubar::new();
        assert_eq!(bar.height, 28.0);
        assert!(bar.visible);
        assert!(!bar.os_menu_active);
        assert_eq!(bar.app_name, "TontooOS");
        assert!(bar.show_clock);
        assert!(bar.show_wifi);
        assert!(bar.show_battery);
    }

    #[test]
    fn set_app_name() {
        let mut bar = Menubar::new();
        bar.set_app_name("Firefox");
        assert_eq!(bar.app_name, "Firefox");
    }

    #[test]
    fn toggle_os_menu() {
        let mut bar = Menubar::new();
        assert!(!bar.os_menu_active);
        bar.toggle_os_menu();
        assert!(bar.os_menu_active);
        bar.toggle_os_menu();
        assert!(!bar.os_menu_active);
    }

    #[test]
    fn hidden_menubar_produces_no_commands() {
        let mut bar = Menubar::new();
        bar.visible = false;
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);
        assert!(cmds.is_empty());
    }

    #[test]
    fn visible_menubar_starts_with_glass_panel() {
        let bar = Menubar::new();
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);
        assert!(!cmds.is_empty());
        match &cmds[0] {
            DrawCommand::GlassPanel {
                width,
                height,
                alpha,
                ..
            } => {
                assert_eq!(*width, 1920.0);
                assert_eq!(*height, 28.0);
                assert_eq!(*alpha, 0.8);
            }
            other => panic!("Expected GlassPanel, got {:?}", other),
        }
    }

    #[test]
    fn dark_theme_uses_zero_milkiness() {
        let bar = Menubar::new();
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);
        if let DrawCommand::GlassPanel { milkiness, .. } = &cmds[0] {
            assert_eq!(*milkiness, 0.0);
        }
    }

    #[test]
    fn light_theme_uses_milky_glass() {
        let bar = Menubar::new();
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Light);
        if let DrawCommand::GlassPanel { milkiness, .. } = &cmds[0] {
            assert_eq!(*milkiness, 0.3);
        }
    }

    #[test]
    fn os_menu_toggle_adds_dropdown_commands() {
        let mut bar = Menubar::new();
        bar.toggle_os_menu();
        let cmds = bar.to_draw_commands(1920.0, ColorScheme::Dark);

        // Should contain the menu background rect + menu item texts.
        let menu_rects: Vec<_> = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { x, y, .. } if *x == 8.0 && *y == 28.0))
            .collect();
        assert!(!menu_rects.is_empty(), "Expected menu background rect");
    }

    #[test]
    fn menubar_items_render() {
        let item = MenubarItem::new("File").with_icon("📄");
        assert_eq!(item.label, "File");
        assert_eq!(item.icon.as_deref(), Some("📄"));
    }
}
