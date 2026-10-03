use crate::TontooCompositor;

use smithay::{
    desktop::{layer_map_for_output, LayerSurface as DesktopLayerSurface},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    utils::SERIAL_COUNTER,
    wayland::shell::wlr_layer::{
        Layer, LayerSurface, LayerSurfaceConfigure, WlrLayerShellHandler, WlrLayerShellState,
    },
};

impl WlrLayerShellHandler for TontooCompositor {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        tracing::info!(
            "New layer surface: namespace={}, layer={:?}",
            namespace,
            layer
        );

        let output_ref = output
            .as_ref()
            .and_then(|o| Output::from_resource(o))
            .or_else(|| self.space.outputs().next().cloned());

        if let Some(output) = output_ref {
            let desktop_surface = DesktopLayerSurface::new(surface, namespace);
            // Read the keyboard request before the surface is moved into the
            // layer map: an interactive surface (the dock's LaunchPad grid)
            // takes keyboard focus while it is mapped.
            let wants_keyboard = desktop_surface.can_receive_keyboard_focus();
            let wl_surface = desktop_surface.wl_surface().clone();
            let mut map = layer_map_for_output(&output);
            if let Err(e) = map.map_layer(&desktop_surface) {
                tracing::error!("Failed to map layer surface: {:?}", e);
            }
            self.pending_redraw = true;
            if wants_keyboard {
                self.focus_layer_surface(&wl_surface);
            }
        } else {
            tracing::warn!("No output available for layer surface");
        }
    }

    fn ack_configure(&mut self, _surface: WlSurface, _configure: LayerSurfaceConfigure) {
        self.pending_redraw = true;
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        tracing::info!("Layer surface destroyed");
        // A destroyed layer surface must not keep the keyboard: hand it back
        // to the active window so typing continues where it left off.
        if let Some(focused) = self.focused_surface.clone() {
            if focused == *surface.wl_surface() {
                self.focus_last_window();
            }
        }
        if let Some(output) = self.space.outputs().next().cloned() {
            let mut map = layer_map_for_output(&output);
            let desktop_layers: Vec<_> = map.layers().cloned().collect();
            for dl in &desktop_layers {
                if dl.wl_surface() == surface.wl_surface() {
                    map.unmap_layer(dl);
                    break;
                }
            }
        }
        self.pending_redraw = true;
    }
}

impl TontooCompositor {
    /// Give the keyboard to a layer surface that asked for it.
    ///
    /// Layer surfaces are otherwise never focused: `input.rs` only focuses
    /// space windows on click. A modal overlay that owns keyboard
    /// interactivity (the LaunchPad grid with its search field) therefore
    /// takes focus as soon as it is mapped.
    fn focus_layer_surface(&mut self, wl_surface: &WlSurface) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            tracing::warn!("No keyboard available for the layer surface");
            return;
        };
        // Remember the window that had the keyboard so it can be restored
        // when this surface closes.
        if self
            .focused_surface
            .as_ref()
            .is_some_and(|current| current != wl_surface)
        {
            self.last_window_focus = self.focused_surface.clone();
        }
        let serial = SERIAL_COUNTER.next_serial();
        keyboard.set_focus(self, Some(wl_surface.clone()), serial);
        self.focused_surface = Some(wl_surface.clone());
        tracing::info!("Layer surface took keyboard focus");
    }

    /// Return the keyboard to the window that had it before a layer surface
    /// took over, or drop it when there was none.
    fn focus_last_window(&mut self) {
        let target = self.last_window_focus.take();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        keyboard.set_focus(self, target.clone(), serial);
        self.focused_surface = target;
        self.pending_redraw = true;
    }
}
