use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use smithay::{
    backend::renderer::{
        element::{
            surface::WaylandSurfaceRenderElement,
            texture::{TextureBuffer, TextureRenderElement},
            Element, Id, Kind, RenderElement,
        },
        gles::{GlesRenderer, GlesTexture},
        utils::CommitCounter,
    },
    input::pointer::{CursorImageStatus, CursorImageSurfaceData},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{user_data::UserDataMap, Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform},
};

use smithay::backend::allocator::Fourcc;

use crate::config::ColorScheme;

pub struct WallpaperElement(pub TextureRenderElement<GlesTexture>);
pub struct CursorTextureElement(pub TextureRenderElement<GlesTexture>);
pub struct DockBarElement(pub TextureRenderElement<GlesTexture>);
pub struct WindowShadowElement(pub TextureRenderElement<GlesTexture>);
pub struct WindowBorderElement(pub TextureRenderElement<GlesTexture>);
pub struct WindowControlsElement(pub TextureRenderElement<GlesTexture>);
pub struct WindowTitlebarElement(pub TextureRenderElement<GlesTexture>);

impl Element for WallpaperElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for WallpaperElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for CursorTextureElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for CursorTextureElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for DockBarElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for DockBarElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for WindowShadowElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for WindowShadowElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for WindowBorderElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for WindowBorderElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for WindowControlsElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for WindowControlsElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

pub struct TontooUiTextureElement(pub TextureRenderElement<GlesTexture>);

impl Element for WindowTitlebarElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for WindowTitlebarElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

impl Element for TontooUiTextureElement {
    fn id(&self) -> &Id { self.0.id() }
    fn current_commit(&self) -> CommitCounter { self.0.current_commit() }
    fn src(&self) -> Rectangle<f64, Buffer> { self.0.src() }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> { self.0.geometry(scale) }
    fn kind(&self) -> Kind { self.0.kind() }
}

impl RenderElement<GlesRenderer> for TontooUiTextureElement {
    fn draw(
        &self,
        frame: &mut <GlesRenderer as smithay::backend::renderer::RendererSuper>::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), <GlesRenderer as smithay::backend::renderer::RendererSuper>::Error> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(&self.0, frame, src, dst, damage, opaque_regions, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<smithay::backend::renderer::element::UnderlyingStorage<'_>> {
        self.0.underlying_storage(renderer)
    }
}

smithay::backend::renderer::element::render_elements! {
    pub TontooRenderElements<=GlesRenderer>;
    Space=smithay::desktop::space::SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>,
    Wallpaper=WallpaperElement,
    DockBar=DockBarElement,
    CursorTexture=CursorTextureElement,
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
    WindowShadow=WindowShadowElement,
    WindowBorder=WindowBorderElement,
    WindowControls=WindowControlsElement,
    WindowTitlebar=WindowTitlebarElement,
    TontooUi=TontooUiTextureElement,
}

// ---------------------------------------------------------------------------
// Built-in fallback cursor (32x32 arrow, white with black outline)
// ---------------------------------------------------------------------------

/// A hardcoded 32x32 arrow cursor used when no XCursor theme is available.
/// Pixel data is RGBA (R, G, B, A).
const FALLBACK_CURSOR_WIDTH: u32 = 32;
const FALLBACK_CURSOR_HEIGHT: u32 = 32;
const FALLBACK_CURSOR_HOTSPOT_X: u32 = 2;
const FALLBACK_CURSOR_HOTSPOT_Y: u32 = 2;

// Speed-based cursor magnification (macOS-style)
const SPEED_THRESHOLD: f64 = 800.0;
const SPEED_MAX: f64 = 2000.0;
const MAX_SCALE: f64 = 2.5;
const RESTORE_DELAY_MS: u64 = 300;
const SCALE_LERP: f64 = 0.15;

fn fallback_cursor_image() -> &'static XCursorImage {
    static CACHE: OnceLock<XCursorImage> = OnceLock::new();
    CACHE.get_or_init(|| {
        let rgba = &include_bytes!("../assets/fallback_cursor_32x32.rgba")[..];
        XCursorImage {
            width: FALLBACK_CURSOR_WIDTH,
            height: FALLBACK_CURSOR_HEIGHT,
            hotspot_x: FALLBACK_CURSOR_HOTSPOT_X,
            hotspot_y: FALLBACK_CURSOR_HOTSPOT_Y,
            rgba: rgba.to_vec(),
        }
    })
}

// ---------------------------------------------------------------------------
// XCursor theme loader
// ---------------------------------------------------------------------------

/// Represents a single loaded cursor image (one size from an XCursor file).
#[derive(Clone)]
struct XCursorImage {
    width: u32,
    height: u32,
    hotspot_x: u32,
    hotspot_y: u32,
    rgba: Vec<u8>,
}

/// TontooOS primary cursor theme name.
fn theme_name_for_scheme(scheme: ColorScheme) -> &'static str {
    match scheme {
        ColorScheme::Dark => "MacTahoe-dark-cursors",
        ColorScheme::Light => "MacTahoe-cursors",
    }
}

/// Fallback cursor theme names searched in order if primary theme is missing.
const FALLBACK_THEMES: &[&str] = &[
    "breeze_cursors",
    "Breeze_Snow",
    "Adwaita",
    "DMZ-White",
    "Vanilla-DMZ",
    "default",
];

/// Loads named cursors from system XCursor theme directories.
struct XCursorLoader {
    theme_name: String,
    theme_size: u32,
    theme_dirs_cache: Option<Vec<PathBuf>>,
    cache: HashMap<String, Option<XCursorImage>>,
    fallback_used: bool,
}

impl XCursorLoader {
    fn new(scheme: ColorScheme) -> Self {
        let theme_name = theme_name_for_scheme(scheme).to_string();
        let theme_size: u32 = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(24);

        tracing::info!("XCursor theme: {} (size {})", theme_name, theme_size);

        Self {
            theme_name,
            theme_size,
            theme_dirs_cache: None,
            cache: HashMap::new(),
            fallback_used: false,
        }
    }

    fn set_scheme(&mut self, scheme: ColorScheme) {
        let new_name = theme_name_for_scheme(scheme).to_string();
        if self.theme_name != new_name {
            tracing::info!("Cursor theme switched: {} -> {}", self.theme_name, new_name);
            self.theme_name = new_name;
            self.theme_dirs_cache = None;
            self.cache.clear();
            self.fallback_used = false;
        }
    }

    fn build_theme_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        let add_theme = |dirs: &mut Vec<PathBuf>, name: &str| {
            if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
                for d in data_home.split(':') {
                    dirs.push(Path::new(d).join("icons").join(name));
                }
            }
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join(".local/share/icons").join(name));
                dirs.push(home.join(".icons").join(name));
            }
            if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
                for d in data_dirs.split(':') {
                    dirs.push(Path::new(d).join("icons").join(name));
                }
            }
            dirs.push(PathBuf::from("/usr/share/icons").join(name));
        };

        add_theme(&mut dirs, &self.theme_name);
        dirs.push(PathBuf::from("/usr/share/icons/default"));

        dirs
    }

    fn theme_dirs(&mut self) -> &[PathBuf] {
        if self.theme_dirs_cache.is_none() {
            let dirs = self.build_theme_dirs();
            self.theme_dirs_cache = Some(dirs);
        }
        self.theme_dirs_cache.as_ref().unwrap()
    }

    fn rebuild_with_fallback(&mut self) {
        let mut dirs = self.build_theme_dirs();

        for fallback in FALLBACK_THEMES {
            if *fallback == self.theme_name {
                continue;
            }
            if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
                for d in data_home.split(':') {
                    dirs.push(Path::new(d).join("icons").join(fallback));
                }
            }
            if let Some(home) = dirs::home_dir() {
                dirs.push(home.join(".local/share/icons").join(fallback));
            }
            if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
                for d in data_dirs.split(':') {
                    dirs.push(Path::new(d).join("icons").join(fallback));
                }
            }
            dirs.push(PathBuf::from("/usr/share/icons").join(fallback));
        }

        self.theme_dirs_cache = Some(dirs);
        self.fallback_used = true;
        tracing::info!(
            "Cursor theme '{}' not found, fallback themes enabled",
            self.theme_name
        );
    }

    fn cursor_candidates(name: &str) -> Vec<&str> {
        let mut candidates = vec![name];
        let aliases = match name {
            "left_ptr" => vec!["default", "arrow"],
            "hand2" => vec!["hand1", "pointer", "pointing_hand"],
            "text" => vec!["xterm", "ibeam"],
            "sb_h_double_arrow" => vec!["col-resize", "h_double_arrow"],
            "sb_v_double_arrow" => vec!["row-resize", "v_double_arrow"],
            "crosshair" => vec!["cross", "tcross"],
            "not-allowed" => vec!["no-drop", "crossed_circle", "forbidden"],
            "watch" => vec!["wait", "progress"],
            "grab" => vec!["openhand", "grabbing"],
            "grabbing" => vec!["closedhand"],
            _ => vec![],
        };
        candidates.extend(aliases);
        candidates
    }

    fn load(&mut self, name: &str) -> Option<XCursorImage> {
        if let Some(cached) = self.cache.get(name) {
            return cached.clone();
        }

        let desired_size = self.theme_size;
        for candidate in Self::cursor_candidates(name) {
            for dir in self.theme_dirs() {
                let cursor_path = dir.join("cursors").join(candidate);
                if !cursor_path.exists() {
                    continue;
                }
                let images = Self::parse_xcursor_file(&cursor_path);
                if let Some(image) = Self::pick_best(&images, desired_size).cloned() {
                    self.cache.insert(name.to_string(), Some(image.clone()));
                    return Some(image);
                }
            }
        }

        if !self.fallback_used {
            tracing::warn!(
                "Cursor '{}' not found in theme '{}', trying fallback themes",
                name,
                self.theme_name
            );
            self.rebuild_with_fallback();
            return self.load(name);
        }

        tracing::warn!(
            "Cursor '{}' not found in any theme (tried primary '{}' and {:?} fallbacks)",
            name,
            self.theme_name,
            FALLBACK_THEMES
        );
        self.cache.insert(name.to_string(), None);
        None
    }

    fn parse_xcursor_file(path: &Path) -> Vec<XCursorImage> {
        let data = match fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Failed to read cursor file {:?}: {}", path, e);
                return vec![];
            }
        };

        let images = match xcursor::parser::parse_xcursor(&data) {
            Some(imgs) => imgs,
            None => {
                tracing::warn!("Failed to parse xcursor {:?}", path);
                return vec![];
            }
        };

        images
            .into_iter()
            .map(|img| XCursorImage {
                width: img.width,
                height: img.height,
                hotspot_x: img.xhot,
                hotspot_y: img.yhot,
                rgba: img.pixels_rgba,
            })
            .collect()
    }

    fn pick_best(images: &[XCursorImage], desired: u32) -> Option<&XCursorImage> {
        if images.is_empty() {
            return None;
        }
        images
            .iter()
            .min_by_key(|img| (img.width as i64 - desired as i64).unsigned_abs())
    }
}

// ---------------------------------------------------------------------------
// CursorState
// ---------------------------------------------------------------------------

pub struct CursorState {
    pub surface: Option<WlSurface>,
    pub visible: bool,
    loader: XCursorLoader,
    active_named: Option<String>,
    cached_named_name: Option<String>,
    cached_named_buffer: Option<TextureBuffer<GlesTexture>>,
    cached_fallback_buffer: Option<TextureBuffer<GlesTexture>>,
    scale_factor: f64,
    target_scale: f64,
    last_pointer_pos: Option<Point<f64, Logical>>,
    last_time: Option<Instant>,
    restore_timer: Option<Instant>,
}

impl CursorState {
    pub fn new(scheme: ColorScheme) -> Self {
        CursorState {
            surface: None,
            visible: true,
            loader: XCursorLoader::new(scheme),
            active_named: Some("left_ptr".to_string()),
            cached_named_name: None,
            cached_named_buffer: None,
            cached_fallback_buffer: None,
            scale_factor: 1.0,
            target_scale: 1.0,
            last_pointer_pos: None,
            last_time: None,
            restore_timer: None,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: ColorScheme) {
        self.loader.set_scheme(scheme);
        self.cached_named_name = None;
        self.cached_named_buffer = None;
        self.cached_fallback_buffer = None;
    }

    pub fn reset_visibility(&mut self) {
        self.visible = true;
        if self.active_named.is_none() {
            self.active_named = Some("left_ptr".to_string());
        }
    }

    pub fn update_speed(&mut self, new_pos: Point<f64, Logical>) {
        let now = Instant::now();

        if let (Some(last_pos), Some(last_time)) = (self.last_pointer_pos, self.last_time) {
            let elapsed = now.duration_since(last_time).as_secs_f64();
            if elapsed > 0.001 {
                let dx = new_pos.x - last_pos.x;
                let dy = new_pos.y - last_pos.y;
                let speed = (dx * dx + dy * dy).sqrt() / elapsed;

                if speed > SPEED_THRESHOLD {
                    let normalized =
                        ((speed - SPEED_THRESHOLD) / (SPEED_MAX - SPEED_THRESHOLD)).min(1.0);
                    self.target_scale = 1.0 + normalized * (MAX_SCALE - 1.0);
                    self.restore_timer = None;
                } else if self.restore_timer.is_none() && self.target_scale > 1.0 {
                    self.restore_timer = Some(now);
                }
            }
        }

        let diff = self.target_scale - self.scale_factor;
        self.scale_factor += diff * SCALE_LERP;
        if (self.scale_factor - 1.0).abs() < 0.001 {
            self.scale_factor = 1.0;
        }

        if let Some(timer) = self.restore_timer {
            if now.duration_since(timer).as_millis() >= RESTORE_DELAY_MS as u128 {
                self.target_scale = 1.0;
                self.restore_timer = None;
            }
        }

        self.last_pointer_pos = Some(new_pos);
        self.last_time = Some(now);
    }

    pub fn handle_cursor_image(&mut self, image: CursorImageStatus) {
        match image {
            CursorImageStatus::Hidden => {
                self.visible = false;
                self.surface = None;
            }
            CursorImageStatus::Surface(ref surface) => {
                self.surface = Some(surface.clone());
                self.active_named = None;
                self.visible = true;
            }
            CursorImageStatus::Named(ref icon) => {
                let name = icon.name().to_string();
                if self.active_named.as_deref() != Some(&name) {
                    self.active_named = Some(name);
                    self.cached_named_name = None;
                }
                self.surface = None;
                self.visible = true;
            }
        }
    }

    pub fn get_cursor_element(
        &mut self,
        renderer: &mut GlesRenderer,
        pointer_pos: Point<f64, Logical>,
    ) -> Option<CursorRenderElement> {
        if !self.visible {
            return None;
        }

        let pos = pointer_pos.to_physical(1.0);

        // 1. Client-provided cursor surface
        if let Some(ref surface) = self.surface {
            if let Some(elem) = create_cursor_from_surface(renderer, surface, pos) {
                return Some(CursorRenderElement::Surface(elem));
            }
        }

        // 2. Named cursor from XCursor theme (TextureBuffer-cached)
        if let Some(ref name) = self.active_named.clone() {
            if let Some(ref xcursor_image) = self.loader.load(name) {
                if self.cached_named_name.as_deref() != Some(name) {
                    tracing::info!("Cursor: creating TextureBuffer for '{}'", name);
                    match TextureBuffer::from_memory(
                        renderer,
                        &xcursor_image.rgba,
                        Fourcc::Abgr8888,
                        (xcursor_image.width as i32, xcursor_image.height as i32),
                        false,
                        1,
                        Transform::Normal,
                        None,
                    ) {
                        Ok(buf) => {
                            self.cached_named_name = Some(name.clone());
                            self.cached_named_buffer = Some(buf);
                            tracing::info!("Cursor: TextureBuffer created for '{}'", name);
                        }
                        Err(e) => {
                            tracing::error!(
                                "Cursor: failed to create TextureBuffer for '{}': {:?}",
                                name,
                                e
                            );
                        }
                    }
                }
                if let Some(ref buffer) = self.cached_named_buffer {
                    let _base_hotspot = Point::<i32, Physical>::from((
                        xcursor_image.hotspot_x as i32,
                        xcursor_image.hotspot_y as i32,
                    ));
                    let scaled_hotspot = Point::<i32, Physical>::from((
                        (xcursor_image.hotspot_x as f64 * self.scale_factor) as i32,
                        (xcursor_image.hotspot_y as f64 * self.scale_factor) as i32,
                    ));
                    let cursor_pos = pos - scaled_hotspot.to_f64();
                    let size = Size::<i32, Logical>::from((
                        (xcursor_image.width as f64 * self.scale_factor) as i32,
                        (xcursor_image.height as f64 * self.scale_factor) as i32,
                    ));
                    let elem = TextureRenderElement::from_texture_buffer(
                        cursor_pos,
                        buffer,
                        None,
                        None,
                        Some(size),
                        Kind::Unspecified,
                    );
                    return Some(CursorRenderElement::Texture(elem));
                }
            }
        }

        // 3. Built-in fallback cursor (TextureBuffer-cached)
        if self.cached_fallback_buffer.is_none() {
            let fb = fallback_cursor_image();
            match TextureBuffer::from_memory(
                renderer,
                &fb.rgba,
                Fourcc::Abgr8888,
                (fb.width as i32, fb.height as i32),
                false,
                1,
                Transform::Normal,
                None,
            ) {
                Ok(buf) => {
                    self.cached_fallback_buffer = Some(buf);
                    tracing::info!("Cursor: created fallback TextureBuffer");
                }
                Err(e) => {
                    tracing::error!("Cursor: failed to create fallback TextureBuffer: {:?}", e);
                    return None;
                }
            }
        }
        if let Some(ref buffer) = self.cached_fallback_buffer {
            let fb = fallback_cursor_image();
            let scaled_hotspot =
                Point::<i32, Physical>::from((
                    (fb.hotspot_x as f64 * self.scale_factor) as i32,
                    (fb.hotspot_y as f64 * self.scale_factor) as i32,
                ));
            let cursor_pos = pos - scaled_hotspot.to_f64();
            let size = Size::<i32, Logical>::from((
                (fb.width as f64 * self.scale_factor) as i32,
                (fb.height as f64 * self.scale_factor) as i32,
            ));
            let elem = TextureRenderElement::from_texture_buffer(
                cursor_pos,
                buffer,
                None,
                None,
                Some(size),
                Kind::Unspecified,
            );
            return Some(CursorRenderElement::Texture(elem));
        }

        None
    }
}

pub enum CursorRenderElement {
    Surface(WaylandSurfaceRenderElement<GlesRenderer>),
    Texture(TextureRenderElement<GlesTexture>),
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn create_cursor_from_surface(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    pos: Point<f64, Physical>,
) -> Option<WaylandSurfaceRenderElement<GlesRenderer>> {
    smithay::wayland::compositor::with_states(surface, |states| {
        let hotspot = states
            .data_map
            .get::<CursorImageSurfaceData>()
            .map(|m| m.lock().unwrap().hotspot)
            .unwrap_or_default();

        let cursor_pos = pos - hotspot.to_f64().to_physical(1.0);

        WaylandSurfaceRenderElement::<GlesRenderer>::from_surface(
            renderer,
            surface,
            states,
            cursor_pos,
            1.0,
            Kind::Cursor,
        )
        .ok()
        .flatten()
    })
}
