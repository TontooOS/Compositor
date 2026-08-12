//! Handler for the `tontoo_ui` custom Wayland protocol.
//!
//! This module manages widget trees sent by client applications for
//! server-side rendering.  The compositor owns the full rendering
//! pipeline — clients never upload buffers or draw pixels.

use std::collections::HashMap;

use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;

use crate::protocol;
use crate::widget_tree::FlatWidget;

// ---------------------------------------------------------------------------
// Color scheme (matches the protocol wire values)
// ---------------------------------------------------------------------------

/// Color scheme preference, matching the `set_color_scheme` request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TontooColorScheme {
    Dark = 0,
    Light = 1,
}

impl Default for TontooColorScheme {
    fn default() -> Self {
        Self::Dark
    }
}

impl TontooColorScheme {
    /// Convert from the wire `uint` value.
    pub fn from_wire(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Dark),
            1 => Some(Self::Light),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Glass / visual effect configuration
// ---------------------------------------------------------------------------

/// Configuration for the frosted-glass (blur + tint) effect applied behind
/// a tontoo_ui surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlassConfig {
    /// Intensity of the frosted-glass tint (`0.0` – `1.0`).
    pub milkiness: f32,
    /// Background alpha behind the glass (`0.0` – `1.0`).
    pub alpha: f32,
    /// Gaussian blur radius in pixels (`0.0` – `50.0`).
    pub sigma: f32,
}

impl Default for GlassConfig {
    fn default() -> Self {
        Self {
            milkiness: 0.5,
            alpha: 0.6,
            sigma: 20.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-surface state
// ---------------------------------------------------------------------------

/// The compositor-side state for a single `tontoo_ui_surface`.
#[derive(Debug, Clone)]
pub struct TontooUiSurfaceState {
    /// Human-readable title (used for window management / accessibility).
    pub title: String,
    /// Desired width in logical pixels.
    pub width: i32,
    /// Desired height in logical pixels.
    pub height: i32,
    /// Optional frosted-glass effect.  `None` means the effect is disabled.
    pub glass: Option<GlassConfig>,
    /// Preferred color scheme for this surface.
    pub color_scheme: TontooColorScheme,
    /// Raw bytes of the serialized widget tree.
    pub widget_data: Vec<u8>,
    /// Parsed widget tree from the last `update_widget_tree` call.
    pub parsed_tree: Vec<FlatWidget>,
    /// The Wayland `wl_surface` id this state is associated with.
    pub wl_surface_id: ObjectId,
    /// Whether the surface has been mapped (shown) by the compositor.
    pub mapped: bool,
    /// The protocol resource handle — needed to send events back to the client.
    pub surface_resource: Option<protocol::tontoo_ui::tontoo_ui_surface::TontooUiSurface>,
    /// Last hovered widget node (for hover event deduplication).
    pub last_hovered_node: Option<usize>,
}

impl TontooUiSurfaceState {
    /// Create a new surface state with sensible defaults.
    pub fn new(wl_surface_id: ObjectId) -> Self {
        Self {
            title: String::new(),
            width: 800,
            height: 600,
            glass: None,
            color_scheme: TontooColorScheme::default(),
            widget_data: Vec::new(),
            parsed_tree: Vec::new(),
            wl_surface_id,
            mapped: false,
            surface_resource: None,
            last_hovered_node: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Global UI state (held on TontooCompositor)
// ---------------------------------------------------------------------------

/// Top-level state for the `tontoo_ui_manager` protocol.
///
/// Stored as a field on [`TontooCompositor`](crate::state::TontooCompositor).
#[derive(Debug, Default)]
pub struct TontooUiState {
    /// All active tontoo_ui surfaces, keyed by the `tontoo_ui_surface` object id.
    pub surfaces: HashMap<ObjectId, TontooUiSurfaceState>,
}

impl TontooUiState {
    /// Register a brand-new surface and return a mutable reference to it.
    pub fn insert_surface(&mut self, id: ObjectId) -> &mut TontooUiSurfaceState {
        self.surfaces
            .entry(id.clone())
            .or_insert_with(|| TontooUiSurfaceState::new(id))
    }

    /// Remove a surface by its object id.  Returns the removed state if present.
    pub fn remove_surface(&mut self, id: &ObjectId) -> Option<TontooUiSurfaceState> {
        self.surfaces.remove(id)
    }

    /// Look up a surface by its object id.
    pub fn get_surface(&self, id: &ObjectId) -> Option<&TontooUiSurfaceState> {
        self.surfaces.get(id)
    }

    /// Mutable lookup.
    pub fn get_surface_mut(&mut self, id: &ObjectId) -> Option<&mut TontooUiSurfaceState> {
        self.surfaces.get_mut(id)
    }

    /// Look up the surface state associated with a given `wl_surface`.
    pub fn find_by_wl_surface(&self, wl_surface: &WlSurface) -> Option<&TontooUiSurfaceState> {
        let target = wl_surface.id();
        self.surfaces.values().find(|s| s.wl_surface_id == target)
    }

    /// Mutable version of [`find_by_wl_surface`].
    pub fn find_by_wl_surface_mut(
        &mut self,
        wl_surface: &WlSurface,
    ) -> Option<&mut TontooUiSurfaceState> {
        let target = wl_surface.id();
        self.surfaces
            .values_mut()
            .find(|s| s.wl_surface_id == target)
    }

    /// Iterator over all surface states.
    pub fn surfaces(&self) -> impl Iterator<Item = &TontooUiSurfaceState> {
        self.surfaces.values()
    }

    /// Mutable iterator over all surface states.
    pub fn surfaces_mut(&mut self) -> impl Iterator<Item = &mut TontooUiSurfaceState> {
        self.surfaces.values_mut()
    }
}

// ---------------------------------------------------------------------------
// Request handler helpers
//
// These are called from the future `Dispatch<TontooUiSurface, …>` and
// `GlobalDispatch<…, TontooUiState>` implementations once wayland-scanner
// is wired up.  They operate purely on the state stored above so that the
// protocol logic is testable in isolation.
// ---------------------------------------------------------------------------

/// Handle a `get_tontoo_ui_surface` request on the manager global.
pub fn handle_get_surface(
    state: &mut TontooUiState,
    surface_id: ObjectId,
) -> &mut TontooUiSurfaceState {
    tracing::debug!("tontoo_ui_manager: new surface {:?}", surface_id);
    state.insert_surface(surface_id)
}

/// Handle a `pong` request — the client responded to our ping.
pub fn handle_pong(_state: &mut TontooUiState, _surface_id: &ObjectId, _serial: u32) {
    // Currently a no-op; in the future we could track liveness.
    tracing::trace!("tontoo_ui_manager: pong received");
}

/// Handle a `set_title` request.
pub fn handle_set_title(state: &mut TontooUiState, surface_id: &ObjectId, title: String) {
    if let Some(surface) = state.get_surface_mut(surface_id) {
        tracing::debug!("tontoo_ui: set_title \"{}\"", title);
        surface.title = title;
    }
}

/// Handle a `set_size` request.
pub fn handle_set_size(state: &mut TontooUiState, surface_id: &ObjectId, width: i32, height: i32) {
    if let Some(surface) = state.get_surface_mut(surface_id) {
        tracing::debug!("tontoo_ui: set_size {}x{}", width, height);
        surface.width = width.max(1);
        surface.height = height.max(1);
    }
}

/// Handle a `set_glass` request.
pub fn handle_set_glass(
    state: &mut TontooUiState,
    surface_id: &ObjectId,
    milkiness: f32,
    alpha: f32,
    sigma: f32,
) {
    if let Some(surface) = state.get_surface_mut(surface_id) {
        if milkiness == 0.0 && alpha == 0.0 && sigma == 0.0 {
            tracing::debug!("tontoo_ui: glass disabled");
            surface.glass = None;
        } else {
            tracing::debug!(
                "tontoo_ui: glass milkiness={} alpha={} sigma={}",
                milkiness,
                alpha,
                sigma
            );
            surface.glass = Some(GlassConfig {
                milkiness,
                alpha,
                sigma,
            });
        }
    }
}

/// Handle an `update_widget_tree` request.
pub fn handle_update_widget_tree(state: &mut TontooUiState, surface_id: &ObjectId, nodes: Vec<u8>) {
    if let Some(surface) = state.get_surface_mut(surface_id) {
        tracing::debug!("tontoo_ui: update_widget_tree {} bytes", nodes.len());
        surface.parsed_tree = crate::widget_tree::parse_widget_tree(&nodes).unwrap_or_default();
        surface.widget_data = nodes;
    }
}

/// Handle a `set_color_scheme` request.
pub fn handle_set_color_scheme(state: &mut TontooUiState, surface_id: &ObjectId, scheme: u32) {
    if let Some(surface) = state.get_surface_mut(surface_id) {
        match TontooColorScheme::from_wire(scheme) {
            Some(s) => {
                tracing::debug!("tontoo_ui: set_color_scheme {:?}", s);
                surface.color_scheme = s;
            }
            None => {
                tracing::warn!("tontoo_ui: unknown color scheme value {}", scheme);
            }
        }
    }
}

/// Handle a `request_close` request from the client.
pub fn handle_request_close(state: &mut TontooUiState, surface_id: &ObjectId) {
    if let Some(surface) = state.get_surface(surface_id) {
        tracing::debug!(
            "tontoo_ui: client requested close for \"{}\"",
            surface.title
        );
    }
    // The compositor decides whether to honour the close request.
    // For now we just log it; the actual destruction happens through
    // the normal Wayland surface lifecycle.
}

/// Handle a `request_minimize` request from the client.
pub fn handle_request_minimize(state: &mut TontooUiState, surface_id: &ObjectId) {
    if let Some(surface) = state.get_surface(surface_id) {
        tracing::debug!(
            "tontoo_ui: client requested minimize for \"{}\"",
            surface.title
        );
    }
}

/// Handle a `request_maximize` request from the client.
pub fn handle_request_maximize(state: &mut TontooUiState, surface_id: &ObjectId) {
    if let Some(surface) = state.get_surface(surface_id) {
        tracing::debug!(
            "tontoo_ui: client requested maximize for \"{}\"",
            surface.title
        );
    }
}

/// Handle surface destruction (called when the `tontoo_ui_surface` object is
/// destroyed or the associated `wl_surface` is destroyed).
pub fn handle_surface_destroy(state: &mut TontooUiState, surface_id: &ObjectId) {
    if let Some(removed) = state.remove_surface(surface_id) {
        tracing::debug!("tontoo_ui: surface destroyed (\"{}\")", removed.title);
    }
}

// ---------------------------------------------------------------------------
// Event-sending helpers
// ---------------------------------------------------------------------------

impl TontooUiSurfaceState {
    pub fn send_widget_clicked(&self, node_id: u32) {
        if let Some(resource) = &self.surface_resource {
            resource.widget_clicked(node_id);
        }
    }

    pub fn send_widget_hovered(&self, node_id: u32) {
        if let Some(resource) = &self.surface_resource {
            resource.widget_hovered(node_id);
        }
    }

    pub fn send_key_event(&self, key: u32, state: u32) {
        if let Some(resource) = &self.surface_resource {
            resource.key_event(key, state);
        }
    }

    pub fn send_configure(&self, width: i32, height: i32) {
        if let Some(resource) = &self.surface_resource {
            resource.configure(width, height);
        }
    }

    pub fn send_close(&self) {
        if let Some(resource) = &self.surface_resource {
            resource.close();
        }
    }
}

// ---------------------------------------------------------------------------
// Wayland Dispatch / GlobalDispatch implementations
// ---------------------------------------------------------------------------

use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New};

/// Data associated with the tontoo_ui_manager global.
pub struct TontooUiManagerGlobalData;

// ── GlobalDispatch for the manager global ──

impl GlobalDispatch<protocol::tontoo_ui::tontoo_ui_manager::TontooUiManager, TontooUiManagerGlobalData>
    for crate::TontooCompositor
{
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<protocol::tontoo_ui::tontoo_ui_manager::TontooUiManager>,
        _global_data: &TontooUiManagerGlobalData,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let _manager = data_init.init(resource, ());
        tracing::debug!("tontoo_ui_manager: bound by client");
    }
}

// ── Dispatch for manager requests ──

impl Dispatch<protocol::tontoo_ui::tontoo_ui_manager::TontooUiManager, ()>
    for crate::TontooCompositor
{
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &protocol::tontoo_ui::tontoo_ui_manager::TontooUiManager,
        request: protocol::tontoo_ui::tontoo_ui_manager::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            protocol::tontoo_ui::tontoo_ui_manager::Request::GetTontooUiSurface { id } => {
                let surface = data_init.init(id, ());
                let surface_id = surface.id();
                let surface_state = state.tontoo_ui.insert_surface(surface_id);
                surface_state.surface_resource = Some(surface);
                tracing::debug!("tontoo_ui_manager: new surface created");
            }
            protocol::tontoo_ui::tontoo_ui_manager::Request::Pong { serial } => {
                super::tontoo_ui::handle_pong(&mut state.tontoo_ui, &ObjectId::null(), serial);
            }
        }
    }
}

// ── Dispatch for surface requests ──

impl Dispatch<protocol::tontoo_ui::tontoo_ui_surface::TontooUiSurface, ()>
    for crate::TontooCompositor
{
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &protocol::tontoo_ui::tontoo_ui_surface::TontooUiSurface,
        request: protocol::tontoo_ui::tontoo_ui_surface::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let surface_id = resource.id();
        match request {
            protocol::tontoo_ui::tontoo_ui_surface::Request::SetTitle { title } => {
                super::tontoo_ui::handle_set_title(&mut state.tontoo_ui, &surface_id, title);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::SetSize { width, height } => {
                super::tontoo_ui::handle_set_size(&mut state.tontoo_ui, &surface_id, width, height);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::SetGlass { milkiness, alpha, sigma } => {
                super::tontoo_ui::handle_set_glass(&mut state.tontoo_ui, &surface_id, milkiness as f32, alpha as f32, sigma as f32);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::UpdateWidgetTree { nodes } => {
                super::tontoo_ui::handle_update_widget_tree(&mut state.tontoo_ui, &surface_id, nodes);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::SetColorScheme { scheme } => {
                super::tontoo_ui::handle_set_color_scheme(&mut state.tontoo_ui, &surface_id, scheme);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::RequestClose => {
                super::tontoo_ui::handle_request_close(&mut state.tontoo_ui, &surface_id);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::RequestMinimize => {
                super::tontoo_ui::handle_request_minimize(&mut state.tontoo_ui, &surface_id);
            }
            protocol::tontoo_ui::tontoo_ui_surface::Request::RequestMaximize => {
                super::tontoo_ui::handle_request_maximize(&mut state.tontoo_ui, &surface_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_object_id() -> ObjectId {
        // We can't easily construct a real ObjectId without a display, so
        // use a placeholder.  For unit tests this is sufficient.
        ObjectId::from_non_null_pointer(0x1 as *mut _)
    }

    #[test]
    fn insert_and_remove_surface() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());
        assert!(state.get_surface(&id).is_some());

        state.remove_surface(&id);
        assert!(state.get_surface(&id).is_none());
    }

    #[test]
    fn set_title() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());

        handle_set_title(&mut state, &id, "My Window".into());
        assert_eq!(state.get_surface(&id).unwrap().title, "My Window");
    }

    #[test]
    fn set_size_clamps_minimum() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());

        handle_set_size(&mut state, &id, -10, 0);
        let s = state.get_surface(&id).unwrap();
        assert_eq!(s.width, 1);
        assert_eq!(s.height, 1);
    }

    #[test]
    fn glass_enabled_and_disabled() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());

        handle_set_glass(&mut state, &id, 0.7, 0.8, 25.0);
        assert!(state.get_surface(&id).unwrap().glass.is_some());

        handle_set_glass(&mut state, &id, 0.0, 0.0, 0.0);
        assert!(state.get_surface(&id).unwrap().glass.is_none());
    }

    #[test]
    fn update_widget_tree() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());

        let data = br#"{"type":"VBox","children":[]}"#.to_vec();
        handle_update_widget_tree(&mut state, &id, data.clone());

        let s = state.get_surface(&id).unwrap();
        assert_eq!(s.widget_data, data);
    }

    #[test]
    fn color_scheme() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        state.insert_surface(id.clone());

        assert_eq!(
            state.get_surface(&id).unwrap().color_scheme,
            TontooColorScheme::Dark
        );

        handle_set_color_scheme(&mut state, &id, 1);
        assert_eq!(
            state.get_surface(&id).unwrap().color_scheme,
            TontooColorScheme::Light
        );

        // Invalid value should not change the scheme
        handle_set_color_scheme(&mut state, &id, 99);
        assert_eq!(
            state.get_surface(&id).unwrap().color_scheme,
            TontooColorScheme::Light
        );
    }

    #[test]
    fn get_surface_creates_on_first_call() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();

        assert!(state.get_surface(&id).is_none());
        handle_get_surface(&mut state, id.clone());
        assert!(state.get_surface(&id).is_some());
    }

    #[test]
    fn surface_destruction() {
        let mut state = TontooUiState::default();
        let id = dummy_object_id();
        handle_get_surface(&mut state, id.clone());

        handle_surface_destroy(&mut state, &id);
        assert!(state.get_surface(&id).is_none());
    }
}
