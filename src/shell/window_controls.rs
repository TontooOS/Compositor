//! macOS-style window controls (traffic light buttons).
//!
//! Renders the classic red/yellow/green "traffic light" buttons at the
//! top-left corner of each window, exactly like macOS.
//!
//! - **Red** (close): #FE5B51
//! - **Yellow** (minimize): #E6C02A
//! - **Green** (maximize): #51C329
//!
//! On hover over the button group, the symbols ×, −, + appear inside
//! the dots (matching macOS behavior).

use crate::config::ColorScheme;

// ═══════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════

/// Diameter of each traffic light dot in logical pixels.
pub const DOT_SIZE: f32 = 12.0;

/// Horizontal spacing between dot centers.
pub const DOT_SPACING: f32 = 8.0;

/// Left padding from the window edge to the first dot.
pub const LEFT_PADDING: f32 = 12.0;

/// Top padding from the window edge to the dot centers.
pub const TOP_PADDING: f32 = 14.0;

/// Total width of the traffic light area (for hit testing).
pub fn total_width() -> f32 {
    LEFT_PADDING + DOT_SIZE * 3.0 + DOT_SPACING * 2.0 + LEFT_PADDING
}

/// Total height of the traffic light area (for hit testing).
pub fn total_height() -> f32 {
    TOP_PADDING + DOT_SIZE + 4.0
}

// ═══════════════════════════════════════════════════════════════
// TrafficLightHit — result of a hit test
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLightAction {
    Close,
    Minimize,
    Maximize,
}

// ═══════════════════════════════════════════════════════════════
// WindowControls
// ═══════════════════════════════════════════════════════════════

/// State for window traffic light buttons.
#[derive(Debug, Clone)]
pub struct WindowControls {
    /// Whether the pointer is currently hovering over the button group.
    pub hovered: bool,
}

impl WindowControls {
    pub fn new() -> Self {
        Self { hovered: false }
    }

    /// Hit-test a pointer position relative to the window's top-left corner.
    ///
    /// Returns `Some(TrafficLightAction)` if the click is on a button,
    /// or `None` if it's outside all buttons.
    pub fn hit_test(&self, rel_x: f32, rel_y: f32) -> Option<TrafficLightAction> {
        let y = rel_y - TOP_PADDING;
        let x = rel_x - LEFT_PADDING;

        if y < 0.0 || y > DOT_SIZE || x < 0.0 {
            return None;
        }

        // Red (close)
        let red_x = 0.0;
        if x >= red_x && x < red_x + DOT_SIZE {
            return Some(TrafficLightAction::Close);
        }

        // Yellow (minimize)
        let yellow_x = DOT_SIZE + DOT_SPACING;
        if x >= yellow_x && x < yellow_x + DOT_SIZE {
            return Some(TrafficLightAction::Minimize);
        }

        // Green (maximize)
        let green_x = (DOT_SIZE + DOT_SPACING) * 2.0;
        if x >= green_x && x < green_x + DOT_SIZE {
            return Some(TrafficLightAction::Maximize);
        }

        None
    }

    /// Check if a pointer position is within the traffic light area (for hover).
    pub fn is_in_area(rel_x: f32, rel_y: f32) -> bool {
        rel_x >= 0.0 && rel_x <= total_width() && rel_y >= 0.0 && rel_y <= total_height()
    }
}

impl Default for WindowControls {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Traffic light pixel data generation
// ═══════════════════════════════════════════════════════════════

/// Color values for the traffic lights (byte order matches memory layout: R, G, B, A).
pub(crate) fn close_color(scheme: ColorScheme) -> [u8; 4] {
    match scheme {
        ColorScheme::Dark => [0xFE, 0x5B, 0x51, 0xFF], // #FE5B51 RGBA
        ColorScheme::Light => [0xFE, 0x5B, 0x51, 0xFF],
    }
}

pub(crate) fn minimize_color(scheme: ColorScheme) -> [u8; 4] {
    match scheme {
        ColorScheme::Dark => [0xE6, 0xC0, 0x2A, 0xFF], // #E6C02A RGBA
        ColorScheme::Light => [0xE6, 0xC0, 0x2A, 0xFF],
    }
}

pub(crate) fn maximize_color(scheme: ColorScheme) -> [u8; 4] {
    match scheme {
        ColorScheme::Dark => [0x51, 0xC3, 0x29, 0xFF], // #51C329 RGBA
        ColorScheme::Light => [0x51, 0xC3, 0x29, 0xFF],
    }
}

/// Inactive traffic light colors (grayed out, when window is not focused).
pub(crate) fn close_color_inactive(scheme: ColorScheme) -> [u8; 4] {
    match scheme {
        ColorScheme::Dark => [0x66, 0x66, 0x66, 0xFF], // #666666 RGBA
        ColorScheme::Light => [0xAA, 0xAA, 0xAA, 0xFF],
    }
}

pub(crate) fn minimize_color_inactive(scheme: ColorScheme) -> [u8; 4] {
    close_color_inactive(scheme)
}

pub(crate) fn maximize_color_inactive(scheme: ColorScheme) -> [u8; 4] {
    close_color_inactive(scheme)
}

/// Generate pixel data for a single circular traffic light dot.
///
/// Returns `(pixels, width, height)` in ABGR format.
pub fn create_traffic_light_dot(
    size: i32,
    color: [u8; 4],
) -> Vec<u8> {
    let su = size as u32;
    let radius = su as f64 / 2.0;
    let center = radius;
    let mut data = vec![0u8; (su * su * 4) as usize];

    for y in 0..su {
        for x in 0..su {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            let i = ((y * su + x) * 4) as usize;
            if dist <= radius {
                // Anti-alias the edge
                let alpha = if dist > radius - 1.0 {
                    ((radius - dist) * 255.0) as u8
                } else {
                    255
                };
                data[i] = color[0];     // R
                data[i + 1] = color[1]; // G
                data[i + 2] = color[2]; // B
                data[i + 3] = alpha;    // A
            } else {
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 0;
            }
        }
    }

    data
}

/// Generate pixel data for the hover symbols (×, −, +) drawn on top of dots.
///
/// Returns `(pixels, width, height)` — white symbols on transparent background.
pub fn create_traffic_light_symbol(
    size: i32,
    symbol: char,
) -> Vec<u8> {
    let su = size as u32;
    let mut data = vec![0u8; (su * su * 4) as usize];
    let center_x = su as f32 / 2.0;
    let center_y = su as f32 / 2.0;
    let line_len = su as f32 * 0.3;
    let line_width = 1.5;

    for y in 0..su {
        for x in 0..su {
            let fx = x as f32;
            let fy = y as f32;
            let mut inside = false;

            match symbol {
                'x' => {
                    // X shape: two diagonal lines
                    let d1 = ((fx - center_x) - (fy - center_y)).abs();
                    let d2 = ((fx - center_x) + (fy - center_y)).abs();
                    let in_range = (fx - center_x).abs() <= line_len
                        && (fy - center_y).abs() <= line_len;
                    inside = in_range && (d1 < line_width || d2 < line_width);
                }
                '-' => {
                    // Horizontal line
                    let in_y = (fy - center_y).abs() < line_width;
                    let in_x = (fx - center_x).abs() <= line_len;
                    inside = in_y && in_x;
                }
                '+' => {
                    // Plus shape
                    let h_line = (fy - center_y).abs() < line_width
                        && (fx - center_x).abs() <= line_len;
                    let v_line = (fx - center_x).abs() < line_width
                        && (fy - center_y).abs() <= line_len;
                    inside = h_line || v_line;
                }
                _ => {}
            }

            let i = ((y * su + x) * 4) as usize;
            if inside {
                // Dark color for symbols (visible on colored dots)
                data[i] = 0x20;     // B
                data[i + 1] = 0x20; // G
                data[i + 2] = 0x20; // R
                data[i + 3] = 180;  // A (semi-transparent)
            }
        }
    }

    data
}
