//! macOS-style Dock shell component.
//!
//! Renders a glass panel at the bottom of the screen with evenly-spaced icons
//! that magnify under the cursor using gaussian distance falloff and spring
//! interpolation for smooth transitions. Running applications display a small
//! indicator dot beneath their icon.

use crate::animation::Animation;
use crate::widget_renderer::{Color, DrawCommand};

use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════

/// Base height of the dock in logical pixels.
const BASE_DOCK_HEIGHT: f32 = 78.0;

/// Horizontal gap between icons inside the dock.
const ICON_GAP: f32 = 12.0;

/// Default icon size (unscaled) in logical pixels.
const ICON_SIZE: f32 = 48.0;

/// Maximum magnification scale applied to the closest icon.
const MAX_MAGNIFICATION: f32 = 1.5;

/// Gaussian sigma controlling the spread of the magnification effect.
/// Larger values make the magnification curve wider.
const MAGNIFICATION_SIGMA: f32 = 120.0;

/// Spring constant for smooth magnification interpolation.
/// Higher values make the spring stiffer (faster convergence).
const SPRING_STIFFNESS: f32 = 12.0;

/// Duration of a single icon bounce cycle.
const BOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Peak vertical displacement of a bounce animation in logical pixels.
const BOUNCE_AMPLITUDE: f32 = 16.0;

/// Number of oscillation cycles in a bounce.
const BOUNCE_CYCLES: f32 = 2.0;

/// Glass panel visual constants.
const GLASS_MILKINESS: f32 = 1.0;
const GLASS_ALPHA: f32 = 0.32;
const GLASS_CORNER_RADIUS: f32 = 22.0;

/// Running-app indicator dot.
const DOT_RADIUS: f32 = 3.0;
const DOT_COLOR: Color = Color::new(0.85, 0.85, 0.87, 0.9);
const DOT_GAP: f32 = 4.0;

// ═══════════════════════════════════════════════════════════════
// DockIcon
// ═══════════════════════════════════════════════════════════════

/// A single application icon pinned to or running in the Dock.
#[derive(Debug, Clone)]
pub struct DockIcon {
    /// Human-readable name (used as fallback label and for lookup).
    pub name: String,
    /// Optional filesystem path to the icon texture.
    pub icon_path: Option<String>,
    /// Whether the application is currently running.
    pub is_running: bool,
}

// ═══════════════════════════════════════════════════════════════
// DockAnimation
// ═══════════════════════════════════════════════════════════════

/// Per-frame animation state for the Dock.
#[derive(Debug)]
pub struct DockAnimation {
    /// Current magnification scale for each icon (0.0 = baseline, 1.0 = fully
    /// magnified). Index-correlated with `Dock::icons`.
    pub magnification: Vec<f32>,
    /// Target magnification values computed by `compute_magnification`.
    /// `tick()` relaxes the current values toward these targets via spring
    /// interpolation.
    pub target_magnification: Vec<f32>,
    /// Optional bounce animation per icon slot. `None` = idle.
    pub bounce_animations: Vec<Option<Animation>>,
}

impl DockAnimation {
    fn new() -> Self {
        Self {
            magnification: Vec::new(),
            target_magnification: Vec::new(),
            bounce_animations: Vec::new(),
        }
    }

    /// Resize internal vectors to match `icon_count`, preserving existing
    /// values where possible.
    fn sync_len(&mut self, icon_count: usize) {
        // Grow — new entries start at 0.0 / None.
        while self.magnification.len() < icon_count {
            self.magnification.push(0.0);
        }
        while self.target_magnification.len() < icon_count {
            self.target_magnification.push(0.0);
        }
        while self.bounce_animations.len() < icon_count {
            self.bounce_animations.push(None);
        }
        // Shrink.
        self.magnification.truncate(icon_count);
        self.target_magnification.truncate(icon_count);
        self.bounce_animations.truncate(icon_count);
    }
}

// ═══════════════════════════════════════════════════════════════
// Dock
// ═══════════════════════════════════════════════════════════════

/// The macOS-style application Dock anchored to the bottom of the screen.
#[derive(Debug)]
pub struct Dock {
    /// Icons currently pinned / running in the Dock, left-to-right.
    pub icons: Vec<DockIcon>,
    /// Rendered height of the dock panel in logical pixels.
    pub height: f32,
    /// Whether the dock is currently visible.
    pub visible: bool,
    /// Index of the icon under the mouse cursor, if any.
    pub hover_index: Option<usize>,
    /// Name of the currently active/focused application (matches DockIcon.name).
    pub active_app: Option<String>,
    /// Animation state for magnification and bounce.
    pub animation_state: DockAnimation,
}

impl Dock {
    // ── Construction ────────────────────────────────────────

    /// Create an empty dock.
    pub fn new() -> Self {
        Self {
            icons: Vec::new(),
            height: BASE_DOCK_HEIGHT,
            visible: true,
            hover_index: None,
            active_app: None,
            animation_state: DockAnimation::new(),
        }
    }

    // ── Icon management ─────────────────────────────────────

    /// Pin a new application icon to the right end of the Dock.
    pub fn add_icon(&mut self, name: &str) {
        // Avoid duplicates.
        if self.icons.iter().any(|i| i.name == name) {
            return;
        }
        self.icons.push(DockIcon {
            name: name.to_string(),
            icon_path: None,
            is_running: false,
        });
        self.animation_state.sync_len(self.icons.len());
    }

    /// Remove an icon by name. Returns `true` if an icon was removed.
    pub fn remove_icon(&mut self, name: &str) -> bool {
        let before = self.icons.len();
        self.icons.retain(|i| i.name != name);
        if self.icons.len() < before {
            // Adjust hover index.
            if let Some(h) = self.hover_index {
                if h >= self.icons.len() {
                    self.hover_index = None;
                }
            }
            self.animation_state.sync_len(self.icons.len());
            true
        } else {
            false
        }
    }

    /// Update which icon the mouse is hovering over.
    pub fn set_hover(&mut self, index: Option<usize>) {
        self.hover_index = index;
    }

    /// Set the active/focused application by name (matches DockIcon.name).
    pub fn set_active_app(&mut self, name: &str) {
        self.active_app = Some(name.to_string());
        // Mark the matching icon as running.
        for icon in &mut self.icons {
            if icon.name == name {
                icon.is_running = true;
            }
        }
    }

    /// Clear the active app (no window focused).
    pub fn clear_active_app(&mut self) {
        self.active_app = None;
    }

    /// Trigger a bounce animation on the icon with the given name.
    pub fn bounce_icon(&mut self, name: &str) {
        if let Some(pos) = self.icons.iter().position(|i| i.name == name) {
            self.animation_state.bounce_animations[pos] = Some(Animation::new(BOUNCE_DURATION));
        }
    }

    /// Find the index of an icon by name.
    pub fn icon_index(&self, name: &str) -> Option<usize> {
        self.icons.iter().position(|i| i.name == name)
    }

    // ── Per-frame update ────────────────────────────────────

    /// Advance all animations by `dt` seconds and run the spring-based
    /// magnification interpolation.
    pub fn tick(&mut self, dt: f32) {
        if !self.visible {
            return;
        }

        let count = self.icons.len();
        if count == 0 {
            return;
        }

        // Tick bounce animations.
        let dt_dur = Duration::from_secs_f32(dt);
        for slot in &mut self.animation_state.bounce_animations {
            if let Some(anim) = slot {
                anim.elapsed += dt_dur;
                if let Some(dur) = anim.duration {
                    if anim.elapsed >= dur {
                        anim.elapsed = dur;
                        anim.running = false;
                    }
                }
                if !anim.running {
                    *slot = None;
                }
            }
        }

        // Spring interpolation toward the target magnification.
        //
        // `compute_magnification` writes targets into `target_magnification`;
        // here we relax the current values toward them with exponential decay.
        let stiffness = SPRING_STIFFNESS * dt;
        for i in 0..count {
            let target = self.animation_state.target_magnification[i];
            let current = &mut self.animation_state.magnification[i];
            *current += (target - *current) * stiffness;
            // Snap to zero when close enough to avoid perpetual micro-movement.
            if (*current - target).abs() < 0.001 {
                *current = target;
            }
        }
    }

    /// Returns `true` while the dock is still visibly moving, i.e. any bounce
    /// animation is running or a magnification spring has not reached its
    /// target yet. The render pump uses this to keep ticking at frame rate
    /// only while an animation actually needs frames.
    pub fn is_animating(&self) -> bool {
        if !self.visible {
            return false;
        }
        if self.animation_state.bounce_animations.iter().any(|a| a.is_some()) {
            return true;
        }
        self.animation_state
            .magnification
            .iter()
            .zip(self.animation_state.target_magnification.iter())
            .any(|(current, target)| (*current - *target).abs() >= 0.001)
    }

    /// Compute the target magnification for each icon based on the mouse
    /// cursor's x-position. Icons closest to the cursor scale up to
    /// [`MAX_MAGNIFICATION`] following a gaussian falloff curve.
    pub fn compute_magnification(&mut self, mouse_x: f32, screen_width: f32) {
        let count = self.icons.len();
        if count == 0 || !self.visible {
            return;
        }

        let spacing = self.icon_spacing(screen_width);
        let start_x = self.icon_start_x(screen_width);

        let targets = &mut self.animation_state.target_magnification;
        targets.clear();
        targets.resize(count, 0.0);

        for i in 0..count {
            let icon_center_x = start_x + i as f32 * spacing + ICON_SIZE * 0.5;
            let distance = (mouse_x - icon_center_x).abs();

            // Gaussian: f(d) = exp(-d² / (2σ²))
            let gaussian =
                (-distance * distance / (2.0 * MAGNIFICATION_SIGMA * MAGNIFICATION_SIGMA)).exp();
            targets[i] = gaussian * MAX_MAGNIFICATION;
        }
    }

    // ── Geometry helpers ────────────────────────────────────

    /// Horizontal spacing between icon centers.
    fn icon_spacing(&self, screen_width: f32) -> f32 {
        let count = self.icons.len().max(1) as f32;
        let usable = screen_width - 2.0 * ICON_GAP;
        (usable / count).max(ICON_SIZE + ICON_GAP * 2.0)
    }

    /// X-coordinate of the first icon's left edge.
    fn icon_start_x(&self, screen_width: f32) -> f32 {
        let count = self.icons.len().max(1) as f32;
        let spacing = self.icon_spacing(screen_width);
        let total_width = count * spacing;
        (screen_width - total_width) * 0.5
    }

    /// Effective height of the dock accounting for the tallest magnified icon.
    pub fn effective_height(&self) -> f32 {
        let max_mag = self
            .animation_state
            .magnification
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        let max_icon = ICON_SIZE * (1.0 + max_mag);
        // Panel height + some headroom for magnified icons + dot.
        (BASE_DOCK_HEIGHT + (max_icon - ICON_SIZE) * 0.5 + DOT_GAP + DOT_RADIUS * 2.0)
            .max(BASE_DOCK_HEIGHT)
    }

    // ── Rendering ───────────────────────────────────────────

    /// Flatten the dock into draw commands for the compositor renderer.
    /// `y_offset` translates the entire dock vertically (e.g. to position it at bottom of screen).
    pub fn to_draw_commands(&self, screen_width: f32, y_offset: f32) -> Vec<DrawCommand> {
        if !self.visible || self.icons.is_empty() {
            return Vec::new();
        }

        let mut cmds = Vec::with_capacity(self.icons.len() * 3 + 2);
        let panel_h = self.effective_height();
        let panel_y = y_offset;

        // ── Glass panel background ──────────────────────────
        cmds.push(DrawCommand::GlassPanel {
            x: 0.0,
            y: panel_y,
            width: screen_width,
            height: panel_h,
            milkiness: GLASS_MILKINESS,
            alpha: GLASS_ALPHA,
            corner_radius: GLASS_CORNER_RADIUS,
        });

        let spacing = self.icon_spacing(screen_width);
        let start_x = self.icon_start_x(screen_width);

        for (i, icon) in self.icons.iter().enumerate() {
            let mag = self
                .animation_state
                .magnification
                .get(i)
                .copied()
                .unwrap_or(0.0);
            let scale = 1.0 + mag;
            let icon_w = ICON_SIZE * scale;
            let icon_h = ICON_SIZE * scale;

            let icon_x = start_x + i as f32 * spacing + (spacing - icon_w) * 0.5;

            // Vertical offset so magnified icons grow upward from the baseline.
            let baseline_y = panel_h - DOT_GAP - DOT_RADIUS * 2.0 - 4.0;
            let icon_y = baseline_y - icon_h;

            // ── Bounce vertical offset ─────────────────────
            let bounce_offset = self.bounce_offset(i);

            // ── Icon background rect ───────────────────────
            cmds.push(DrawCommand::Rect {
                x: icon_x,
                y: icon_y + bounce_offset,
                width: icon_w,
                height: icon_h,
                color: Color::new(0.20, 0.20, 0.22, 0.85),
                corner_radius: 10.0 * scale,
            });

            // ── Icon texture or label ──────────────────────
            if let Some(_path) = &icon.icon_path {
                // Texture placeholder — texture_id will be resolved by the
                // asset pipeline in a future integration.
                cmds.push(DrawCommand::Texture {
                    x: icon_x + 4.0 * scale,
                    y: icon_y + bounce_offset + 4.0 * scale,
                    width: icon_w - 8.0 * scale,
                    height: icon_h - 8.0 * scale,
                    texture_id: 0, // placeholder
                });
            } else {
                // Fallback: render the first character of the name as a label.
                let label: String = icon.name.chars().take(2).collect();
                cmds.push(DrawCommand::Text {
                    content: label,
                    x: icon_x + (icon_w - 16.0 * scale) * 0.5,
                    y: icon_y + bounce_offset + (icon_h - 14.0 * scale) * 0.5,
                    font_size: 14.0 * scale,
                    color: Color::new(0.90, 0.90, 0.92, 1.0),
                    max_width: Some(icon_w - 8.0 * scale),
                });
            }

            // ── Running-app indicator dot ──────────────────
            let is_active = self.active_app.as_deref() == Some(icon.name.as_str());
            if icon.is_running || is_active {
                let dot_x = icon_x + (icon_w - DOT_RADIUS * 2.0) * 0.5;
                let dot_y = baseline_y + DOT_GAP;
                cmds.push(DrawCommand::Rect {
                    x: dot_x,
                    y: dot_y + bounce_offset,
                    width: DOT_RADIUS * 2.0,
                    height: DOT_RADIUS * 2.0,
                    color: if is_active {
                        Color::new(0.95, 0.55, 0.15, 1.0) // Orange accent for active
                    } else {
                        DOT_COLOR
                    },
                    corner_radius: DOT_RADIUS,
                });
            }
        }

        cmds
    }

    // ── Private helpers ─────────────────────────────────────

    /// Compute the vertical bounce offset for icon at `index`.
    /// Uses a damped sinusoidal curve over the animation duration.
    pub fn bounce_offset(&self, index: usize) -> f32 {
        let anim = match self.animation_state.bounce_animations.get(index) {
            Some(Some(a)) if a.running => a,
            _ => return 0.0,
        };

        let progress = anim.progress() as f32;
        // Damped sine: A * sin(2π * cycles * t) * (1 - t)
        let envelope = 1.0 - progress;
        let phase = BOUNCE_CYCLES * std::f32::consts::TAU * progress;
        -BOUNCE_AMPLITUDE * phase.sin() * envelope
    }
}

impl Default for Dock {
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
    fn new_dock_is_empty() {
        let dock = Dock::new();
        assert!(dock.icons.is_empty());
        assert!(dock.visible);
        assert_eq!(dock.height, BASE_DOCK_HEIGHT);
    }

    #[test]
    fn add_icon_deduplicates() {
        let mut dock = Dock::new();
        dock.add_icon("Finder");
        dock.add_icon("Finder");
        assert_eq!(dock.icons.len(), 1);
    }

    #[test]
    fn remove_icon_adjusts_hover() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.add_icon("B");
        dock.set_hover(Some(1));
        assert!(dock.remove_icon("B"));
        assert_eq!(dock.hover_index, None);
    }

    #[test]
    fn magnification_computed_for_all_icons() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.add_icon("B");
        dock.add_icon("C");
        dock.compute_magnification(500.0, 1000.0);
        assert_eq!(dock.animation_state.target_magnification.len(), 3);
        // Center icon should have highest magnification.
        let targets = &dock.animation_state.target_magnification;
        assert!(targets[1] >= targets[0]);
        assert!(targets[1] >= targets[2]);
    }

    #[test]
    fn draw_commands_include_glass_panel() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        let cmds = dock.to_draw_commands(1920.0, 0.0);
        assert!(cmds
            .iter()
            .any(|c| matches!(c, DrawCommand::GlassPanel { .. })));
    }

    #[test]
    fn running_icon_produces_dot() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.icons[0].is_running = true;
        let cmds = dock.to_draw_commands(1920.0, 0.0);
        // Expect: glass panel + icon rect + label + dot = 4 commands.
        let dot_count = cmds
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    DrawCommand::Rect {
                        width,
                        height,
                        ..
                    } if (*width - DOT_RADIUS * 2.0).abs() < 0.1
                        && (*height - DOT_RADIUS * 2.0).abs() < 0.1
                )
            })
            .count();
        assert_eq!(dot_count, 1);
    }

    #[test]
    fn bounce_icon_creates_animation() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.bounce_icon("A");
        assert!(dock.animation_state.bounce_animations[0].is_some());
    }

    #[test]
    fn tick_advances_bounce() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.bounce_icon("A");
        let anim = dock.animation_state.bounce_animations[0].as_ref().unwrap();
        assert_eq!(anim.elapsed, Duration::ZERO);

        dock.tick(0.1);
        let anim = dock.animation_state.bounce_animations[0].as_ref().unwrap();
        assert!(anim.elapsed > Duration::ZERO);
    }

    #[test]
    fn empty_dock_produces_no_commands() {
        let dock = Dock::new();
        let cmds = dock.to_draw_commands(1920.0, 0.0);
        assert!(cmds.is_empty());
    }

    #[test]
    fn hidden_dock_produces_no_commands() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.visible = false;
        let cmds = dock.to_draw_commands(1920.0, 0.0);
        assert!(cmds.is_empty());
    }

    #[test]
    fn spring_convergence() {
        let mut dock = Dock::new();
        dock.add_icon("A");
        dock.add_icon("B");
        dock.add_icon("C");

        // Hover over center icon.
        dock.compute_magnification(960.0, 1920.0);

        // Tick many times to let the spring converge.
        for _ in 0..200 {
            dock.tick(0.016);
        }

        // After many ticks the magnification should be close to target.
        for i in 0..3 {
            let diff = (dock.animation_state.magnification[i]
                - dock.animation_state.target_magnification[i])
                .abs();
            assert!(
                diff < 0.05,
                "Icon {i}: current={} target={} diff={}",
                dock.animation_state.magnification[i],
                dock.animation_state.target_magnification[i],
                diff,
            );
        }
    }
}
