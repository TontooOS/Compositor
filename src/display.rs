//! Display settings owned by the compositor: output modes, brightness
//! and night light. Served over the settings socket (`get_displays`,
//! `set_display`); the Settings daemon persists the values.
//!
//! Refresh switching is resolution-preserving by design: only a mode with
//! the current output size can be applied live (the space geometry and
//! all windows stay valid). The DRM surface is recreated with the new
//! mode; anything else (resolution changes) is rejected.
//!
//! Brightness and night light are software overlays rendered above all
//! content just below the cursor: a black quad for dimming, a warm quad
//! for night light. They work on every backend including VMs without a
//! backlight device.

use serde::{Deserialize, Serialize};

use smithay::backend::renderer::{
    element::texture::TextureRenderElement, gles::{GlesRenderer, GlesTexture},
};

use crate::widget_renderer::{Color, WidgetRenderer};
use crate::TontooCompositor;

/// One output mode: resolution plus refresh rate in Hz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayMode {
    pub width: i32,
    pub height: i32,
    pub refresh: u32,
}

/// One output with its modes and current mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub name: String,
    pub modes: Vec<DisplayMode>,
    pub current: Option<DisplayMode>,
}

/// Full display state behind `get_displays`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayState {
    pub outputs: Vec<DisplayInfo>,
    pub brightness: u32,
    pub night_light: bool,
}

/// Partial `set_display` request: every field optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetDisplay {
    pub output: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub refresh: Option<u32>,
    pub brightness: Option<f64>,
    pub night_light: Option<bool>,
}

/// Warm tint alpha of the night light overlay.
pub const NIGHT_LIGHT_ALPHA: f32 = 0.30;

/// Clamp a 0-100 brightness value to a 0.0-1.0 factor.
pub fn brightness_factor(value: f64) -> f32 {
    (value as f32 / 100.0).clamp(0.0, 1.0)
}

/// Parse a `set_display` request with validation. Brightness must be
/// 0-100, refresh 1-1000 Hz, sizes positive.
pub fn parse_set_display(request: &serde_json::Value) -> Result<SetDisplay, String> {
    let number = |key: &str| request.get(key).and_then(|v| v.as_u64());
    let output = request
        .get("output")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let width = number("width")
        .map(|v| v as i32)
        .filter(|_| true)
        .map(|v| {
            if v <= 0 {
                Err(format!("invalid width: {v}"))
            } else {
                Ok(v)
            }
        })
        .transpose()?;
    let height = number("height")
        .map(|v| v as i32)
        .map(|v| {
            if v <= 0 {
                Err(format!("invalid height: {v}"))
            } else {
                Ok(v)
            }
        })
        .transpose()?;
    let refresh = number("refresh")
        .map(|v| v as u32)
        .map(|v| {
            if v == 0 || v > 1000 {
                Err(format!("invalid refresh rate: {v}"))
            } else {
                Ok(v)
            }
        })
        .transpose()?;
    let brightness = request.get("brightness").and_then(|v| v.as_f64()).map(|v| {
        if !(0.0..=100.0).contains(&v) {
            Err(format!("invalid brightness: {v}"))
        } else {
            Ok(v)
        }
    }).transpose()?;
    let night_light = request.get("night_light").and_then(|v| v.as_bool());
    Ok(SetDisplay {
        output,
        width,
        height,
        refresh,
        brightness,
        night_light,
    })
}

/// Fullscreen display overlays for one output: black dim quad for
/// brightness below full plus a warm quad for night light. Pushed above
/// all content but below the cursor. Empty when both are inactive.
pub fn overlay_elements(
    renderer: &mut GlesRenderer,
    width: f32,
    height: f32,
    brightness: f32,
    night_light: bool,
) -> Vec<TextureRenderElement<GlesTexture>> {
    let mut elements = Vec::new();
    if brightness < 0.999 {
        if let Some(elem) = WidgetRenderer::render_rect_cmd(
            renderer,
            0.0,
            0.0,
            width,
            height,
            Color::new(0.0, 0.0, 0.0, 1.0 - brightness.clamp(0.0, 1.0)),
        ) {
            elements.push(elem);
        }
    }
    if night_light {
        if let Some(elem) = WidgetRenderer::render_rect_cmd(
            renderer,
            0.0,
            0.0,
            width,
            height,
            Color::new(1.0, 0.55, 0.25, NIGHT_LIGHT_ALPHA),
        ) {
            elements.push(elem);
        }
    }
    elements
}

/// List outputs with modes and current modes. Empty without backends.
pub fn list_displays(state: &TontooCompositor) -> Vec<DisplayInfo> {
    let mut outputs: Vec<DisplayInfo> = state
        .space
        .outputs()
        .map(|output| {
            let current = output.current_mode().map(|mode| DisplayMode {
                width: mode.size.w,
                height: mode.size.h,
                refresh: (mode.refresh.max(0) / 1000).max(1) as u32,
            });
            DisplayInfo {
                name: output.name(),
                modes: connector_modes(state, output),
                current,
            }
        })
        .collect();
    outputs.sort_by(|a, b| a.name.cmp(&b.name));
    outputs
}

/// Connector modes for an output on the udev backend (empty on winit,
/// where the mode is fixed by the host window).
#[cfg(feature = "udev")]
fn connector_modes(state: &TontooCompositor, output: &smithay::output::Output) -> Vec<DisplayMode> {
    use smithay::reexports::drm::control::Device as _;

    let Some(udev) = state.udev_data.as_ref() else {
        return Vec::new();
    };
    for device in udev.devices.values() {
        let crtc = match device
            .surfaces
            .iter()
            .find(|(_, surface)| surface.output.name() == output.name())
            .map(|(crtc, _)| *crtc)
        {
            Some(crtc) => crtc,
            None => continue,
        };
        let conn_handle = match device
            .known_connectors
            .iter()
            .find(|(_, c)| **c == crtc)
            .map(|(conn, _)| *conn)
        {
            Some(conn) => conn,
            None => continue,
        };
        let conn = match device.drm.get_connector(conn_handle, false) {
            Ok(conn) => conn,
            Err(_) => continue,
        };
        let mut modes: Vec<DisplayMode> = conn
            .modes()
            .iter()
            .map(|m| {
                let (w, h) = m.size();
                DisplayMode {
                    width: w as i32,
                    height: h as i32,
                    refresh: m.vrefresh(),
                }
            })
            .collect();
        modes.sort_by_key(|m| (m.width, m.height, m.refresh));
        modes.dedup();
        return modes;
    }
    Vec::new()
}

/// No connector access without the udev backend.
#[cfg(not(feature = "udev"))]
fn connector_modes(_state: &TontooCompositor, _output: &smithay::output::Output) -> Vec<DisplayMode> {
    Vec::new()
}

/// Apply validated display settings: brightness and night light take
/// effect immediately; a refresh rate switches the output mode live when
/// it matches the current resolution (udev), or must equal the current
/// mode on winit. Returns the effective output name, mode, brightness
/// percent and night light flag.
pub fn apply_display(
    state: &mut TontooCompositor,
    request: &SetDisplay,
) -> Result<(String, Option<DisplayMode>, u32, bool), String> {
    let name = match &request.output {
        Some(name) => {
            if !state.space.outputs().any(|o| &o.name() == name) {
                return Err(format!("unknown output: {name}"));
            }
            name.clone()
        }
        None => state
            .space
            .outputs()
            .next()
            .map(|o| o.name())
            .ok_or_else(|| "no outputs".to_string())?,
    };
    if let Some(brightness) = request.brightness {
        state.display_brightness = brightness_factor(brightness);
    }
    if let Some(night_light) = request.night_light {
        state.display_night_light = night_light;
    }
    let mode = match (request.width, request.height, request.refresh) {
        (None, None, None) => current_mode_of(state, &name),
        (width, height, refresh) => {
            apply_output_mode(state, &name, width, height, refresh)?;
            current_mode_of(state, &name)
        }
    };
    state.request_redraw();
    let brightness = (state.display_brightness * 100.0).round() as u32;
    Ok((name, mode, brightness, state.display_night_light))
}

/// Current mode of an output, if any.
fn current_mode_of(state: &TontooCompositor, name: &str) -> Option<DisplayMode> {
    state.space.outputs().find(|o| o.name() == *name).and_then(|output| {
        output.current_mode().map(|mode| DisplayMode {
            width: mode.size.w,
            height: mode.size.h,
            refresh: (mode.refresh.max(0) / 1000).max(1) as u32,
        })
    })
}

/// Switch an output to a mode. Width/height fall back to the current
/// size; only same-resolution refresh switches apply live.
#[cfg(feature = "udev")]
fn apply_output_mode(
    state: &mut TontooCompositor,
    name: &str,
    width: Option<i32>,
    height: Option<i32>,
    refresh: Option<u32>,
) -> Result<(), String> {
    use smithay::output::Mode;
    use smithay::reexports::drm::control::{connector, crtc, Device as _};
    use smithay::utils::Transform;

    let current = current_mode_of(state, name)
        .ok_or_else(|| format!("output has no current mode: {name}"))?;
    let width = width.unwrap_or(current.width);
    let height = height.unwrap_or(current.height);
    if width != current.width || height != current.height {
        return Err(format!(
            "resolution switching needs a reboot (asked {width}x{height}, running {}x{})",
            current.width, current.height
        ));
    }
    let refresh = refresh.unwrap_or(current.refresh);
    if refresh == current.refresh {
        return Ok(());
    }
    let Some(udev) = state.udev_data.as_mut() else {
        return Err("udev backend not running".to_string());
    };
    // Locate device, CRTC and connector for the output.
    let mut target: Option<(smithay::backend::drm::DrmNode, crtc::Handle, connector::Handle)> = None;
    for (node, device) in &udev.devices {
        for (crtc, surface) in &device.surfaces {
            if surface.output.name() == *name {
                if let Some((conn, _)) = device
                    .known_connectors
                    .iter()
                    .find(|(_, c)| **c == *crtc)
                {
                    target = Some((*node, *crtc, *conn));
                }
            }
        }
    }
    let (node, crtc, conn_handle) = target.ok_or_else(|| format!("no DRM connector for output: {name}"))?;
    let device = udev
        .devices
        .get_mut(&node)
        .ok_or_else(|| format!("no DRM device for output: {name}"))?;
    let conn = device
        .drm
        .get_connector(conn_handle, false)
        .map_err(|e| format!("connector query failed: {e:?}"))?;
    let drm_mode = conn
        .modes()
        .iter()
        .find(|m| {
            let (w, h) = m.size();
            w as i32 == width && h as i32 == height && m.vrefresh() == refresh
        })
        .copied()
        .ok_or_else(|| format!("no {width}x{height} @ {refresh}Hz mode on output: {name}"))?;
    // Recreate the surface with the new mode. Resolution is unchanged,
    // so the space geometry and all windows stay valid.
    let surface = device
        .drm
        .create_surface(crtc, drm_mode, &[conn_handle])
        .map_err(|e| format!("surface recreation failed: {e:?}"))?;
    let allocator = smithay::backend::allocator::gbm::GbmAllocator::new(
        device.gbm.clone(),
        smithay::backend::allocator::gbm::GbmBufferFlags::RENDERING
            | smithay::backend::allocator::gbm::GbmBufferFlags::SCANOUT,
    );
    let exporter =
        smithay::backend::drm::exporter::gbm::GbmFramebufferExporter::new(device.gbm.clone(), smithay::backend::drm::exporter::gbm::NodeFilter::None);
    let compositor = crate::udev::TontooDrmCompositor::new(
        &device.surfaces.get(&crtc).map(|s| s.output.clone()).ok_or_else(|| format!("output vanished: {name}"))?,
        surface,
        None,
        allocator,
        exporter,
        vec![smithay::backend::allocator::Fourcc::Xrgb8888],
        device.renderer_formats.clone(),
        device.drm.cursor_size(),
        Some(device.gbm.clone()),
    )
    .map_err(|e| format!("surface recreation failed: {e:?}"))?;
    let wl_mode = Mode::from(drm_mode);
    if let Some(entry) = device.surfaces.get_mut(&crtc) {
        entry.output.change_current_state(Some(wl_mode), Some(Transform::Normal), None, None);
        entry.compositor = compositor;
    }
    tracing::info!(
        "output {} switched to {}x{} @ {}Hz",
        name,
        drm_mode.size().0,
        drm_mode.size().1,
        drm_mode.vrefresh()
    );
    Ok(())
}

/// Winit outputs are host windows: only the running mode applies.
#[cfg(not(feature = "udev"))]
fn apply_output_mode(
    state: &mut TontooCompositor,
    name: &str,
    width: Option<i32>,
    height: Option<i32>,
    refresh: Option<u32>,
) -> Result<(), String> {
    let current = current_mode_of(state, name)
        .ok_or_else(|| format!("output has no current mode: {name}"))?;
    let width = width.unwrap_or(current.width);
    let height = height.unwrap_or(current.height);
    let refresh = refresh.unwrap_or(current.refresh);
    if width == current.width && height == current.height && refresh == current.refresh {
        return Ok(());
    }
    Err("mode switching is only supported on the udev backend".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_clamps() {
        assert_eq!(brightness_factor(100.0), 1.0);
        assert_eq!(brightness_factor(0.0), 0.0);
        assert_eq!(brightness_factor(50.0), 0.5);
        assert_eq!(brightness_factor(-5.0), 0.0);
        assert_eq!(brightness_factor(140.0), 1.0);
    }

    #[test]
    fn parse_validates_ranges() {
        let ok = parse_set_display(
            &serde_json::json!({"output": "HDMI-1", "refresh": 120, "brightness": 80.0, "night_light": true}),
        )
        .unwrap();
        assert_eq!(ok.output.as_deref(), Some("HDMI-1"));
        assert_eq!(ok.refresh, Some(120));
        assert_eq!(ok.brightness, Some(80.0));
        assert_eq!(ok.night_light, Some(true));
        assert!(parse_set_display(&serde_json::json!({"brightness": 101.0})).is_err());
        assert!(parse_set_display(&serde_json::json!({"brightness": -1.0})).is_err());
        assert!(parse_set_display(&serde_json::json!({"refresh": 0})).is_err());
        assert!(parse_set_display(&serde_json::json!({"refresh": 1001})).is_err());
        assert!(parse_set_display(&serde_json::json!({"width": 0})).is_err());
        assert!(parse_set_display(&serde_json::json!({})).unwrap().output.is_none());
    }
}
