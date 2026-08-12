use crate::TontooCompositor;

use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface as DesktopLayerSurface},
    output::Output,
    reexports::wayland_server::protocol::{wl_output::WlOutput, wl_surface::WlSurface},
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
            let mut map = layer_map_for_output(&output);
            if let Err(e) = map.map_layer(&desktop_surface) {
                tracing::error!("Failed to map layer surface: {:?}", e);
            }
        } else {
            tracing::warn!("No output available for layer surface");
        }
    }

    fn ack_configure(&mut self, _surface: WlSurface, _configure: LayerSurfaceConfigure) {}

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        tracing::info!("Layer surface destroyed");
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
    }
}

delegate_layer_shell!(TontooCompositor);
