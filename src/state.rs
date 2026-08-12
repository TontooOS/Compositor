use crate::protocol;
use std::{ffi::OsString, path::PathBuf, sync::Arc};

use smithay::{
    desktop::{layer_map_for_output, PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Logical, Point, Rectangle},
    wayland::{
        compositor::{with_states, CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{
            wlr_layer::{Layer, WlrLayerShellState},
            xdg::{
                decoration::XdgDecorationState,
                XdgShellState, XdgToplevelSurfaceData,
            },
        },
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use smithay::backend::renderer::{element::texture::TextureBuffer, gles::GlesTexture};

use crate::accessibility::AccessibilitySettings;
use crate::cursor::CursorState;
use crate::handlers::tontoo_ui::TontooUiState;
use crate::handlers::tontoo_ui::TontooUiManagerGlobalData;
use crate::render_cache::RenderCache;
use crate::shell::ShellState;
use crate::texture_cache::TextureCache;
use crate::wallpaper::Wallpaper;
use crate::widget_renderer::WidgetRenderer;

pub struct TontooCompositor {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub layer_shell_state: WlrLayerShellState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<TontooCompositor>,
    pub data_device_state: DataDeviceState,
    pub xdg_decoration_state: XdgDecorationState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,

    pub color_scheme: crate::config::ColorScheme,

    pub accessibility: AccessibilitySettings,

    pub cursor: CursorState,

    pub wallpaper: Option<Wallpaper>,
    pub wallpaper_path: PathBuf,
    pub wallpaper_buffer: Option<TextureBuffer<GlesTexture>>,

    pub texture_cache: TextureCache,

    pub widget_renderer: WidgetRenderer,

    pub animation_manager: crate::animation::AnimationManager,

    pub shell: ShellState,

    /// State for the custom `tontoo_ui` Wayland protocol.
    pub tontoo_ui: TontooUiState,

    /// The currently focused window surface (for 2-click behavior and active app tracking).
    pub focused_surface: Option<WlSurface>,

    /// Cached render textures to avoid recomputing every frame.
    pub render_cache: RenderCache,

    #[cfg(feature = "udev")]
    pub udev_data: Option<crate::udev::UdevData>,
}

impl TontooCompositor {
    pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let popups = PopupManager::default();
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        // Register the tontoo_ui_manager global
        dh.create_global::<crate::TontooCompositor, protocol::tontoo_ui::tontoo_ui_manager::TontooUiManager, TontooUiManagerGlobalData>(
            1,
            TontooUiManagerGlobalData,
        );

        let xdg_decoration_state = XdgDecorationState::new::<TontooCompositor>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");

        seat.add_keyboard(Default::default(), 200, 25).unwrap();
        seat.add_pointer();

        let space = Space::default();

        let color_scheme = crate::config::load_color_scheme();
        crate::config::apply_color_scheme_env(color_scheme);

        let accessibility = AccessibilitySettings::load();

        let wallpaper_path = std::env::var("TONTOO_WALLPAPER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from("/usr/share/tontoo/wallpapers");
                p.push("THAOELAKE");
                p.push("IMAGE.png");
                p
            });

        let wallpaper = Wallpaper::load(&wallpaper_path).ok();
        if wallpaper.is_none() {
            tracing::warn!(
                "Wallpaper not found at {:?}, using solid color",
                wallpaper_path
            );
        }

        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();

        let mut render_cache = RenderCache::new();
        render_cache.load_font();

        Self {
            start_time,
            display_handle: dh,
            space,
            loop_signal,
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            layer_shell_state,
            output_manager_state,
            seat_state,
            data_device_state,
            xdg_decoration_state,
            popups,
            seat,
            color_scheme,
            accessibility,
            cursor: CursorState::new(color_scheme),
            wallpaper,
            wallpaper_path,
            wallpaper_buffer: None,
            texture_cache: TextureCache::new(),
            widget_renderer: WidgetRenderer::new(),
            animation_manager: crate::animation::AnimationManager::new(),
            shell: ShellState::new(),
            tontoo_ui: TontooUiState::default(),
            focused_surface: None,
            render_cache,
            #[cfg(feature = "udev")]
            udev_data: None,
        }
    }

    fn init_wayland_listener(
        display: Display<TontooCompositor>,
        event_loop: &mut EventLoop<Self>,
    ) -> OsString {
        let listening_socket = ListeningSocketSource::new_auto().unwrap();
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("Failed to init wayland event source.");

        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    // Flush outgoing messages to all connected clients.
                    // Without this, clients never receive server responses
                    // (like wl_registry events) and hang without creating surfaces.
                    unsafe {
                        display.get_mut().flush_clients().unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        // Check layer surfaces first (overlay and top layers are above windows)
        if let Some(output) = self.space.outputs().next() {
            let map = layer_map_for_output(output);
            // Check layers from topmost to bottommost: Overlay, Top, Bottom, Background
            for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
                for layer_surface in map.layers_on(layer) {
                    if let Some(geometry) = map.layer_geometry(layer_surface) {
                        let geo: Rectangle<i32, Logical> = geometry;
                        let geo_f64 = geo.to_f64();
                        if geo_f64.contains(pos) {
                            if let Some(wl_surface) = layer_surface.wl_surface().clone().into() {
                                return Some((wl_surface, pos - geo_f64.loc));
                            }
                        }
                    }
                }
            }
        }

        // Then check regular windows
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, (p + location).to_f64()))
            })
    }

    pub fn request_redraw(&mut self) {
        #[cfg(feature = "udev")]
        crate::udev::try_render_all(self);
    }

    /// Tick the animation manager and trigger a redraw if animations are still active.
    pub fn request_redraw_with_animation(&mut self, dt: std::time::Duration) {
        self.animation_manager.tick(dt);
        if self.animation_manager.has_active() {
            #[cfg(feature = "udev")]
            crate::udev::try_render_all(self);
        }
    }

    pub fn set_color_scheme(&mut self, scheme: crate::config::ColorScheme) {
        self.color_scheme = scheme;
        self.cursor.set_color_scheme(scheme);
        crate::config::apply_color_scheme_env(scheme);
        if let Err(e) = crate::config::save_color_scheme(scheme) {
            tracing::error!("Failed to save color scheme: {}", e);
        }
        tracing::info!("Color scheme changed to {:?}", scheme);
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

/// Get the XDG app_id from a Window (set by the client via `set_app_id()`).
pub fn get_app_id(window: &Window) -> Option<String> {
    let surface = window.toplevel()?.wl_surface();
    with_states(surface, |states| {
        let data = states.data_map.get::<XdgToplevelSurfaceData>()?;
        data.lock().ok()?.app_id.clone()
    })
}

/// Get the XDG title from a Window (set by the client via `set_title()`).
pub fn get_window_title(window: &Window) -> Option<String> {
    let surface = window.toplevel()?.wl_surface();
    with_states(surface, |states| {
        let data = states.data_map.get::<XdgToplevelSurfaceData>()?;
        data.lock().ok()?.title.clone()
    })
}
