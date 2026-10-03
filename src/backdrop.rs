//! Desktop backdrop streaming.
//!
//! Wayland never lets a client read the pixels of another surface, so a
//! client rendered window that wants a frosted glass background has to get
//! the desktop from the compositor. This module captures the elements *below*
//! a window once, reads them back and writes them into a memory file the
//! client allocated. Everything past that (blur radius, lens magnification,
//! frost, rim shading) is the client's job, which keeps the compositor out of
//! the per-frame Gaussian blur entirely.
//!
//! See `wiki/Backdrop.md` for the protocol description.

use std::fs::File;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::time::{Duration, Instant};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                surface::WaylandSurfaceRenderElement, texture::TextureBuffer,
                texture::TextureRenderElement, AsRenderElements, Element, Kind, RenderElement,
            },
            gles::{ffi, GlesFrame, GlesRenderbuffer, GlesRenderer, GlesTexture},
            Bind, Frame, Offscreen, Renderer,
        },
    },
    desktop::{space::SpaceRenderElements, Space, Window},
    output::Output,
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::{
        user_data::UserDataMap, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
    },
};


use crate::handlers::tontoo_ui::TontooUiState;
use crate::wallpaper::Wallpaper;

/// Downscale divisor used when the client sends `0`.
pub const DEFAULT_SCALE: u32 = 2;
/// Largest divisor the compositor accepts.
pub const MAX_SCALE: u32 = 4;

/// Safety net for surfaces whose repaints this compositor never sees.
/// Only active while an X11 window overlaps a stream, so a pure Wayland
/// desktop never reaches it.
pub const SELF_HEAL_INTERVAL: Duration = Duration::from_secs(1);

/// Backoff after a capture that could not be produced, so a rect that
/// permanently does not fit the client buffer cannot spin the offscreen
/// pass every frame.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(1);


/// Element type produced by `Space::render_elements_for_output`.
type SpaceElem = SpaceRenderElements<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

/// Clamp a client supplied divisor into the supported range.
pub fn clamp_scale(scale: u32) -> u32 {
    match scale {
        0 => DEFAULT_SCALE,
        s => s.clamp(1, MAX_SCALE),
    }
}

// ---------------------------------------------------------------------------
// Stream state
// ---------------------------------------------------------------------------

/// Why a capture is about to run. Reported in the debug log so a stream over
/// a static desktop can be verified to stay at one capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureReason {
    /// Something explicitly dirtied the stream: a foreign surface repainted
    /// inside the window rect, the wallpaper changed, a window moved.
    Dirty,
    /// The window rect itself differs from the last captured one.
    Moved,
    /// Nothing dirtied the stream but the self-heal cadence elapsed.
    SelfHeal,
}

impl CaptureReason {
    /// Stable short name for the log.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dirty => "dirty",
            Self::Moved => "moved",
            Self::SelfHeal => "self-heal",
        }
    }
}

/// Backdrop subscription of a single `tontoo_ui_surface`.
#[derive(Debug, Clone)]
pub struct BackdropStream {
    /// Last value of the `set_backdrop` request.
    pub enabled: bool,
    /// Downscale divisor, clamped to `1..=MAX_SCALE`.
    pub scale: u32,
    /// Protocol id of the watched `wl_surface`, `0` when unset.
    pub wl_surface_id: u32,
    /// Client shared mapping, `None` before `create_backdrop_buffer`.
    pub buffer: Option<BackdropBuffer>,
    /// Frame counter, incremented on every sent frame.
    pub serial: u32,
    /// Serial waiting for `ack_backdrop`. Blocks the next capture so a slow
    /// client never reads a half-written frame.
    pub pending_ack: Option<u32>,
    /// Last captured rect in output-local physical pixels, `None` before the
    /// first capture.
    pub last_region: Option<Rectangle<i32, Physical>>,
    /// Set when something behind the window changed, cleared on a
    /// successful capture.
    pub dirty: bool,
    /// Blocks a retry after a failed capture.
    pub retry_after: Option<Instant>,
    /// Time of the last capture attempt, drives the self-heal cadence.
    pub last_capture: Option<Instant>,
    /// Number of captures performed, reported in the debug log.
    pub captures: u32,
}

impl Default for BackdropStream {
    fn default() -> Self {
        Self {
            enabled: false,
            scale: DEFAULT_SCALE,
            wl_surface_id: 0,
            buffer: None,
            serial: 0,
            pending_ack: None,
            last_region: None,
            // A fresh stream has never seen a frame, so it captures at once.
            dirty: true,
            retry_after: None,
            last_capture: None,
            captures: 0,
        }
    }
}

impl BackdropStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while the compositor may try to capture this frame.
    pub fn wants_capture(&self) -> bool {
        self.enabled && self.buffer.is_some() && self.pending_ack.is_none()
    }

    /// True while a failed capture is still in its backoff window.
    pub fn in_backoff(&self, now: Instant) -> bool {
        self.retry_after.map(|t| now < t).unwrap_or(false)
    }

    /// Why a fresh frame is needed, or `None` when the current one is still
    /// good.
    ///
    /// Frame damage is deliberately not consulted: it is the union of every
    /// element, including the window itself, so a window repainting its own
    /// glass would recapture on every frame even over a still desktop. See
    /// `wiki/Backdrop.md`.
    ///
    /// `untracked` enables the self-heal cadence and must only be set when a
    /// surface overlaps the region whose repaints this compositor cannot see
    /// (X11, see [`untracked_overlap`]). With a pure Wayland desktop it stays
    /// `false` and a still backdrop is captured exactly once.
    pub fn needs_capture(
        &self,
        region: Rectangle<i32, Physical>,
        now: Instant,
        untracked: bool,
    ) -> Option<CaptureReason> {
        if self.in_backoff(now) {
            return None;
        }
        if self.dirty {
            return Some(CaptureReason::Dirty);
        }
        if self.last_region != Some(region) {
            return Some(CaptureReason::Moved);
        }
        if untracked
            && self
                .last_capture
                .map(|t| now.duration_since(t) >= SELF_HEAL_INTERVAL)
                .unwrap_or(true)
        {
            return Some(CaptureReason::SelfHeal);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Dirty signalling
// ---------------------------------------------------------------------------

/// Mark every stream for recapture. For changes that affect the whole
/// output: wallpaper, crossfade, brightness, night light.
pub fn dirty_all<'a>(streams: &mut impl Iterator<Item = &'a mut BackdropStream>) {
    for stream in streams {
        stream.dirty = true;
    }
}

/// Mark every stream whose last captured rect overlaps `rect`.
///
/// `skip_surface_id` is the protocol id of the surface that just committed:
/// a window is never part of its own backdrop, so its own repaints must not
/// invalidate it. The overlap test keeps unrelated repaints (the menubar
/// clock ticking every second) out of streams they cannot affect.
///
/// Streams that never captured have no `last_region` and are skipped; they
/// capture on their first frame anyway.
pub fn dirty_intersecting<'a>(
    streams: &mut impl Iterator<Item = &'a mut BackdropStream>,
    rect: Rectangle<i32, Physical>,
    skip_surface_id: u32,
) {
    if rect.size.w < 1 || rect.size.h < 1 {
        return;
    }
    for stream in streams {
        if stream.wl_surface_id == skip_surface_id {
            continue;
        }
        let hit = stream
            .last_region
            .map(|region| region.intersection(rect).is_some())
            .unwrap_or(false);
        if hit {
            stream.dirty = true;
        }
    }
}

/// Output that owns `geo`, if any.
fn output_of<'a>(
    space: &'a Space<Window>,
    geo: Rectangle<i32, Logical>,
) -> Option<&'a smithay::output::Output> {
    space
        .outputs()
        .find(|output| space.output_geometry(output).map(|g| g.overlaps(geo)) == Some(true))
}

/// True when an X11 window overlaps `region`.
///
/// X11 surfaces repaint through `XWayland` and never reach
/// `CompositorHandler::commit`, so a stream over one cannot rely on the dirty
/// flag alone. `Window::toplevel` is `None` exactly for X11 windows.
fn untracked_overlap(
    space: &Space<Window>,
    region: Rectangle<i32, Physical>,
    out_loc: Point<i32, Logical>,
    out_scale: f64,
) -> bool {
    space.elements().any(|window| {
        if window.toplevel().is_some() {
            return false;
        }
        space
            .element_geometry(window)
            .map(|geo| {
                region
                    .intersection(physical_region(geo, out_loc, out_scale))
                    .is_some()
            })
            .unwrap_or(false)
    })
}

/// Mark streams dirty after `surface` committed new pixels.
///
/// Resolves the surface to a space window (converted to output-local physical
/// pixels) or to a layer surface, whose geometry is already physical, then
/// dirties every stream that overlaps. Popups and unclaimed surfaces sit above
/// the desktop and are ignored.
///
/// Called from `CompositorHandler::commit` for every frame a client paints,
/// so it early-outs when no stream is subscribed.
pub fn dirty_from_commit(
    tontoo_ui: &mut TontooUiState,
    space: &Space<Window>,
    surface: &WlSurface,
) {
    if !tontoo_ui.surfaces().any(|s| s.backdrop.enabled) {
        return;
    }
    let id = Resource::id(surface).protocol_id();

    // Layer surfaces first: the dock and the menubar are separate outputs'
    // worth of content that shows through glass.
    for output in space.outputs() {
        let map = smithay::desktop::layer_map_for_output(output);
        let Some(layer) =
            map.layer_for_surface(surface, smithay::desktop::WindowSurfaceType::ALL)
        else {
            continue;
        };
        if let Some(geo) = map.layer_geometry(layer) {
            // `layer_geometry` is logical; streams store physical.
            let scale = output.current_scale().fractional_scale();
            dirty_intersecting(
                &mut tontoo_ui.surfaces_mut().map(|s| &mut s.backdrop),
                geo.to_physical_precise_round(scale),
                id,
            );
        }
        return;
    }

    let Some(window) = space
        .elements()
        .find(|w| {
            crate::state::window_wl_surface_any(w)
                .map(|s| Resource::id(&s).protocol_id() == id)
                .unwrap_or(false)
        })
    else {
        return;
    };
    let Some(geo) = space.element_geometry(window) else {
        return;
    };
    let Some(output) = output_of(space, geo) else {
        return;
    };
    let out_scale = output.current_scale().fractional_scale();
    let out_loc = space
        .output_geometry(output)
        .map(|g| g.loc)
        .unwrap_or_default();
    dirty_intersecting(
        &mut tontoo_ui.surfaces_mut().map(|s| &mut s.backdrop),
        physical_region(geo, out_loc, out_scale),
        id,
    );
}

/// One space element's identity and geometry.
pub type GeometryEntry = (u32, Rectangle<i32, Logical>);
/// Snapshot of every space element's protocol id and rect.
///
/// Moving, resizing, maximizing or fullscreening a window produces no buffer
/// commit at all, so `CompositorHandler::commit` cannot see it. Comparing one
/// small vector per frame catches drag, resize, shortcuts, the windows IPC,
/// mapping, unmapping and layer-surface repositioning in a single place.
#[derive(Debug, Default)]
pub struct BackdropGeometry {
    last: Vec<GeometryEntry>,
}

impl BackdropGeometry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a layout snapshot and report whether anything differs from the
    /// previous one.
    ///
    /// The first call always reports `true`: there is no previous layout, so
    /// nothing can be assumed to be unchanged.
    pub fn apply(&mut self, current: Vec<GeometryEntry>) -> bool {
        let changed = current != self.last;
        self.last = current;
        changed
    }

    /// Collect the current layout and report whether anything moved.
    pub fn refresh(&mut self, space: &Space<Window>) -> bool {
        let current: Vec<GeometryEntry> = space
            .elements()
            .filter_map(|window| {
                let surface = crate::state::window_wl_surface_any(window)?;
                let id = surface.id().protocol_id();
                let geo = space.element_geometry(window)?;
                Some((id, geo))
            })
            .collect();
        self.apply(current)
    }
}


// ---------------------------------------------------------------------------
// Shared buffer
// ---------------------------------------------------------------------------

/// A writable mapping of the memory file the client allocated.
pub struct BackdropBuffer {
    file: File,
    map: *mut u8,
    len: usize,
    width: i32,
    height: i32,
    stride: i32,
}

impl std::fmt::Debug for BackdropBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackdropBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl Clone for BackdropBuffer {
    fn clone(&self) -> Self {
        // Share the same file through a second mapping so both sides keep
        // writing and reading one memory region.
        let file = self.file.try_clone().expect("dup backdrop fd");
        Self::map_fd(file, self.width, self.height, self.stride).expect("remap backdrop buffer")
    }
}

impl BackdropBuffer {
    /// Take ownership of `fd` and map `stride * height` bytes read-write.
    ///
    /// Returns `None` for degenerate geometry, a stride below `width * 4`,
    /// or an `mmap` failure. A rejected descriptor is closed again, which is
    /// what the client expects when it retries with a larger buffer.
    pub fn new(fd: RawFd, width: i32, height: i32, stride: i32) -> Option<Self> {
        // SAFETY: the caller passes a descriptor it just received over the
        // wire and hands ownership over with it.
        let file = unsafe { File::from_raw_fd(fd) };
        Self::map_fd(file, width, height, stride)
    }

    fn map_fd(file: File, width: i32, height: i32, stride: i32) -> Option<Self> {
        if width < 1 || height < 1 || stride < width * 4 {
            return None;
        }
        let len = (stride as usize).checked_mul(height as usize)?;
        // SAFETY: plain file mapping, no pointers are dereferenced yet.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return None;
        }
        Some(Self {
            file,
            map: map as *mut u8,
            len,
            width,
            height,
            stride,
        })
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

/// Copy `count` RGBA8 pixels into row `y` starting at column `x`,
    /// advancing the destination by `step` pixels per source pixel.
    fn write_row(&mut self, y: i32, x: i32, src: &[u8], count: usize, step: usize) {
        if y < 0 || y >= self.height || count == 0 || x < 0 || x >= self.width {
            return;
        }
        let stride = self.stride as usize;
        // SAFETY: callers clamp y and x to the mapping bounds and pass a
        // `src` of at least `count` pixels.
        unsafe {
            let dst = self.map.add(y as usize * stride + x as usize * 4);
            if step == 1 {
                // Full resolution: one copy instead of a per-pixel loop.
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, count * 4);
                return;
            }
            for i in 0..count {
                let px = x + i as i32 * step as i32;
                if px < 0 || px >= self.width {
                    continue;
                }
                std::ptr::copy_nonoverlapping(src.as_ptr().add(i * 4), dst.add(px as usize * 4), 4);
            }
        }
    }

    /// Read `count` pixels from row `y` starting at column `x`, advancing
    /// the source by `step` pixels. Test helper for `write_row`.
    #[cfg(test)]
    fn peek(&self, y: i32, x: i32, count: usize, step: usize) -> Vec<u8> {
        let mut out = vec![0u8; count * 4];
        let stride = self.stride as usize;
        // SAFETY: the caller clamps y and x to the mapping bounds.
        unsafe {
            let row = self.map.add(y as usize * stride);
            for i in 0..count {
                let px = x + i as i32 * step as i32;
                if px < 0 || px >= self.width {
                    continue;
                }
                std::ptr::copy_nonoverlapping(row.add(px as usize * 4), out.as_mut_ptr().add(i * 4), 4);
            }
        }
        out
    }
}

impl Drop for BackdropBuffer {

    fn drop(&mut self) {
        // SAFETY: the mapping came from the matching mmap in `map_fd`.
        unsafe {
            libc::munmap(self.map as *mut libc::c_void, self.len);
        }
    }
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// Reusable offscreen render target plus readback staging for the capture.
pub struct BackdropCapture {
    target: Option<GlesRenderbuffer>,
    /// Reused across captures: at full resolution a fresh allocation would be
    /// several megabytes per capture.
    readback: Vec<u8>,
}

impl Default for BackdropCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl BackdropCapture {
    pub fn new() -> Self {
        Self {
            target: None,
            readback: Vec::new(),
        }
    }

    /// Drop the cached target so the next capture reallocates it.
    pub fn invalidate(&mut self) {
        self.target = None;
    }


    fn ensure_target(
        &mut self,
        renderer: &mut GlesRenderer,
        size: Size<i32, Physical>,
    ) -> Option<&mut GlesRenderbuffer> {
        let buffer_size = Size::from((size.w, size.h));
        let needs = match &self.target {
            Some(t) => t.size() != buffer_size,
            None => true,
        };
        if needs {
            self.target = renderer.create_buffer(Fourcc::Xrgb8888, buffer_size).ok();
        }
        self.target.as_mut()
    }
}

/// One captured frame: the window rect inside the shared buffer plus the
/// downscale divisor the client has to apply. All values are physical
/// pixels, matching the client buffer and the client window size.
#[derive(Debug, Clone, Copy)]
pub struct BackdropFrame {
    pub x: i32,
    pub y: i32,
    /// Rect width in physical pixels (the window size).
    pub width: i32,
    /// Rect height in physical pixels.
    pub height: i32,
    pub scale: u32,
}


/// Render everything below `window` into an offscreen target at
/// `1 / scale` resolution and write the read pixels into `buffer`.
///
/// Returns `None` when the window rect is empty or off-output, when the
/// window has no render elements, when the rect does not fit the client
/// buffer, or when target allocation or readback failed.
#[allow(clippy::too_many_arguments)]
pub fn capture(
    scratch: &mut BackdropCapture,
    renderer: &mut GlesRenderer,
    space: &Space<Window>,
    output: &Output,
    window: &Window,
    wallpaper: Option<(&TextureBuffer<GlesTexture>, &Wallpaper, &str, Option<f32>)>,
    buffer: &mut BackdropBuffer,
    scale: u32,
    clear_color: [f32; 4],
) -> Option<BackdropFrame> {
    let out_geo = space.output_geometry(output)?;
    let geo = space.element_geometry(window)?;
    let region = geo.intersection(out_geo)?;
    if region.size.w < 1 || region.size.h < 1 {
        return None;
    }

    let out_scale = output.current_scale().fractional_scale();
    let divisor = clamp_scale(scale) as i32;
    let render_scale = out_scale / divisor as f64;

    // The shared buffer and the client window are both in physical pixels,
    // so the whole protocol is physical: convert the logical rect once here.
    let phys = physical_region(region, out_geo.loc, out_scale);
    let width_phys = phys.size.w.max(1);
    let height_phys = phys.size.h.max(1);
    // Round up so the last sample stays inside the rect: `(rw - 1) *
    // divisor <= width_phys - 1`, which is what the client assumes when it
    // derives `columns = (width - 1) / divisor + 1`.
    let rw = (width_phys + divisor - 1) / divisor;
    let rh = (height_phys + divisor - 1) / divisor;
    let target_size = Size::<i32, Physical>::from((rw, rh));
    let full = Rectangle::from_size(target_size);

    // Buffer-local origin: the client only needs to know how the sampled
    // region lines up with its own window, not where it sits on screen.
    let bx = phys.loc.x;
    let by = phys.loc.y;
    let columns = (rw - 1) * divisor as i32 + 1;
    let rows = (rh - 1) * divisor as i32 + 1;
    if bx < 0
        || by < 0
        || bx + (columns - 1) * divisor as i32 >= buffer.width()
        || by + (rows - 1) * divisor as i32 >= buffer.height()
    {
        return None;
    }


// Staging for the readback, taken out of the cache up front: the bound
    // render target borrows the same struct, so the two cannot be alive at
    // once. Moved back in at the end so the next capture reuses it.
    let mut pixels = std::mem::take(&mut scratch.readback);

    // Everything strictly below the window, in compositor draw order.
    //
    // `render_output` composites front-to-back (it walks the slice with
    // `.rev()`, so index 0 is the topmost element), and
    // `render_elements_for_output` sorts descending by z-index to match. The
    // slice is therefore topmost-first, so what sits *below* the window is the
    // tail after its own contiguous run, not a prefix.
    let own = window.render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
        renderer,
        Point::from((0, 0)),
        Scale::from(out_scale),
        1.0,
    );
    let target_ids: Vec<_> = own.iter().map(|e| e.id()).collect();
    if target_ids.is_empty() {
        return None;
    }
    let space_elems: Vec<SpaceElem> = space.render_elements_for_output(renderer, output, 1.0).ok()?;
    let Some(start) = space_elems
        .iter()
        .position(|e| target_ids.iter().any(|id| *id == e.id()))
    else {
        // Not in this output's element list, so its layering here is unknown.
        // Capturing the whole stack would bake windows in front of it.
        return None;
    };
    let mut end = start;
    while end < space_elems.len() && target_ids.iter().any(|id| *id == space_elems[end].id()) {
        end += 1;
    }
    let below = &space_elems[end..];


    let wallpaper_elems = wallpaper.map(|(buf, wallpaper, fill, alpha)| {
        wallpaper_elements(
            buf,
            wallpaper,
            fill,
            alpha,
            Size::from((out_geo.size.w, out_geo.size.h)),
            render_scale,
        )
    });

    // Offscreen pass. Wallpaper first, then the space prefix; both lists are
    // already bottom-to-top.
    let target = scratch.ensure_target(renderer, target_size)?;
    let cache = UserDataMap::default();
    {
        let mut fb = renderer.bind(target).ok()?;
        let mut frame = renderer.render(&mut fb, target_size, Transform::Normal).ok()?;
        frame.clear(clear_color.into(), &[full]).ok()?;
        if let Some(elems) = &wallpaper_elems {
            for elem in elems {
                draw(&mut frame, elem, Scale::from(render_scale), &cache);
            }
        }
        for elem in below {
            draw(&mut frame, elem, Scale::from(render_scale), &cache);
        }
        let _sync = frame.finish().ok()?;
    }

// Read back while the renderbuffer is still bound. OpenGL hands out the
    // bottom row first, the shared buffer is top-down.
    let needed = (rw as usize) * (rh as usize) * 4;
    if pixels.len() < needed {
        pixels.resize(needed, 0);
    }
    {
        let mut fb = renderer.bind(target).ok()?;
        let mut frame = renderer.render(&mut fb, target_size, Transform::Normal).ok()?;
        frame
            .with_context(|gl| unsafe {
                gl.ReadBuffer(ffi::COLOR_ATTACHMENT0);
                gl.PixelStorei(ffi::PACK_ALIGNMENT, 1);
                gl.ReadPixels(
                    0,
                    0,
                    rw,
                    rh,
                    ffi::RGBA,
                    ffi::UNSIGNED_BYTE,
                    pixels.as_mut_ptr() as *mut libc::c_void,
                );
            })
            .ok()?;
        let _sync = frame.finish().ok()?;
    }

    let step = divisor as usize;
    for row in 0..rh as usize {
        // `pixels` row 0 is the bottom of the region, the shared buffer is
        // top-down, so walk the readback from its last row.
        let src = &pixels[(rh as usize - 1 - row) * rw as usize * 4..];
        buffer.write_row(by + row as i32 * step as i32, bx, src, rw as usize, step);
    }
    // Hand the staging back so the next capture reuses the allocation.
    scratch.readback = pixels;

    Some(BackdropFrame {

        x: bx,
        y: by,
        width: width_phys,
        height: height_phys,
        scale: divisor as u32,
    })
}


/// Draw one element into the offscreen frame, using its own rect as damage.
fn draw<E: RenderElement<GlesRenderer>>(
    frame: &mut GlesFrame<'_, '_>,
    elem: &E,
    scale: Scale<f64>,
    cache: &UserDataMap,
) {
    let dst = elem.geometry(scale);
    if dst.size.w < 1 || dst.size.h < 1 {
        return;
    }
    let _ = elem.draw(frame, elem.src(), dst, &[dst], &[], Some(cache));
}

/// Wallpaper quads translated and scaled into the offscreen target.
///
/// `wallpaper_layout` already returns output-local pixels, so only the
/// downscale factor is applied here.
fn wallpaper_elements(
    buffer: &TextureBuffer<GlesTexture>,
    wallpaper: &Wallpaper,
    fill: &str,
    alpha: Option<f32>,
    out_size: Size<i32, Physical>,
    render_scale: f64,
) -> Vec<TextureRenderElement<GlesTexture>> {
    let (wp_w, wp_h) = wallpaper.size();
    let src: Rectangle<f64, Logical> =
        Rectangle::from_size(Size::from((wp_w as f64, wp_h as f64)));
    crate::wallpaper::wallpaper_layout(wp_w, wp_h, out_size.w, out_size.h, fill)
        .into_iter()
        .map(|quad| {
            TextureRenderElement::from_texture_buffer(
                Point::from((
                    quad.offset.0 * render_scale,
                    quad.offset.1 * render_scale,
                )),
                buffer,
                alpha,
                Some(src),
                Some(Size::from((
                    (quad.size.0 as f64 * render_scale).round().max(1.0) as i32,
                    (quad.size.1 as f64 * render_scale).round().max(1.0) as i32,
                ))),
                Kind::Unspecified,
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Per frame driver
// ---------------------------------------------------------------------------

/// Space window whose `wl_surface` matches the watched protocol id.
fn window_for_stream<'a>(space: &'a Space<Window>, wl_surface_id: u32) -> Option<&'a Window> {
    if wl_surface_id == 0 {
        return None;
    }
    space.elements().find(|window| {
        crate::state::window_wl_surface_any(window)
            .map(|surface| Resource::id(&surface).protocol_id() == wl_surface_id)
            .unwrap_or(false)
    })
}

/// Physical rect of `region` inside its output.
fn physical_region(
    region: Rectangle<i32, Logical>,
    out_loc: Point<i32, Logical>,
    out_scale: f64,
) -> Rectangle<i32, Physical> {
    Rectangle::new(
        Point::from((
            ((region.loc.x - out_loc.x) as f64 * out_scale).round() as i32,
            ((region.loc.y - out_loc.y) as f64 * out_scale).round() as i32,
        )),
        Size::from((
            (region.size.w as f64 * out_scale).round() as i32,
            (region.size.h as f64 * out_scale).round() as i32,
        )),
    )
}

/// Recapture and announce a backdrop frame for every subscribed surface.
///
/// `geometry` carries the per-frame layout snapshot: a change dirties every
/// stream, which is the only way window moves are seen because moving a
/// window produces no buffer commit.
pub fn update_streams(
    tontoo_ui: &mut TontooUiState,
    space: &Space<Window>,
    wallpaper: Option<(&TextureBuffer<GlesTexture>, &Wallpaper, &str, Option<f32>)>,
    reduce_transparency: bool,
    renderer: &mut GlesRenderer,
    output: &Output,
    geometry: &mut BackdropGeometry,
    scratch: &mut BackdropCapture,
    clear_color: [f32; 4],
) {
    if reduce_transparency {
        return;
    }
    let out_scale = output.current_scale().fractional_scale();
    if geometry.refresh(space) {
        dirty_all(&mut tontoo_ui.surfaces_mut().map(|s| &mut s.backdrop));
    }
    if !tontoo_ui
        .surfaces()
        .any(|surface| surface.backdrop.wants_capture())
    {
        return;
    }
    let out_loc = space.output_geometry(output).map(|geo| geo.loc);

    let now = Instant::now();
    let mut captured: Vec<(smithay::reexports::wayland_server::backend::ObjectId, BackdropFrame)> =
        Vec::new();

    for (id, surface) in tontoo_ui.surfaces.iter_mut() {
        if !surface.backdrop.wants_capture() {
            continue;
        }
        let Some(window) = window_for_stream(space, surface.backdrop.wl_surface_id) else {
            continue;
        };
        let Some(region) = space.element_geometry(window) else {
            continue;
        };
        let Some(out_loc) = out_loc else {
            continue;
        };
        let phys = physical_region(region, out_loc, out_scale);
        let untracked = untracked_overlap(space, phys, out_loc, out_scale);
        let Some(reason) = surface.backdrop.needs_capture(phys, now, untracked) else {
            continue;
        };
        let scale = surface.backdrop.scale;
        let Some(buffer) = surface.backdrop.buffer.as_mut() else {
            continue;
        };
        let Some(frame) = capture(
            scratch,
            renderer,
            space,
            output,
            window,
            wallpaper,
            buffer,
            scale,
            clear_color,
        ) else {
            // A rect that never fits the client buffer would otherwise retry
            // the offscreen pass on every frame.
            surface.backdrop.last_capture = Some(now);
            surface.backdrop.retry_after = Some(now + RETRY_INTERVAL);
            continue;
        };
        surface.backdrop.last_region = Some(phys);
        surface.backdrop.last_capture = Some(now);
        surface.backdrop.retry_after = None;
        surface.backdrop.dirty = false;
        surface.backdrop.captures = surface.backdrop.captures.wrapping_add(1);
        tracing::debug!(
            "tontoo_ui: backdrop capture #{} reason={} rect={}x{}+{}+{} scale={}",
            surface.backdrop.captures,
            reason.as_str(),
            frame.width,
            frame.height,
            frame.x,
            frame.y,
            frame.scale,
        );
        captured.push((id.clone(), frame));
    }


    for (id, frame) in captured {
        let Some(surface) = tontoo_ui.surfaces.get_mut(&id) else {
            continue;
        };
        surface.backdrop.serial = surface.backdrop.serial.wrapping_add(1);
        let serial = surface.backdrop.serial;
        surface.backdrop.pending_ack = Some(serial);
        surface.send_backdrop(
            serial,
            frame.x,
            frame.y,
            frame.width,
            frame.height,
            frame.scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logical_rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    fn physical_rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new(Point::from((x, y)), Size::from((w, h)))
    }

    #[test]
    fn scale_is_clamped() {
        assert_eq!(clamp_scale(0), DEFAULT_SCALE);
        assert_eq!(clamp_scale(1), 1);
        assert_eq!(clamp_scale(3), 3);
        assert_eq!(clamp_scale(9), MAX_SCALE);
    }

    #[test]
    fn stream_defaults() {
        let stream = BackdropStream::new();
        assert!(!stream.enabled);
        assert!(!stream.wants_capture());
        assert_eq!(stream.scale, DEFAULT_SCALE);
        assert!(stream.buffer.is_none());
    }

    #[test]
    fn capture_blocked_until_acked() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        assert!(!stream.wants_capture());
        stream.pending_ack = Some(3);
        assert!(!stream.wants_capture());
        stream.pending_ack = None;
        assert!(!stream.wants_capture());
        let buffer = test_buffer(64);
        stream.buffer = Some(buffer);
        assert!(stream.wants_capture());
    }

#[test]
    fn region_change_forces_capture() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = false;
        let region = physical_rect(10, 10, 100, 50);
        stream.last_region = Some(region);
        stream.last_capture = Some(Instant::now());
        assert_eq!(stream.needs_capture(region, Instant::now(), false), None);
        let moved = physical_rect(11, 10, 100, 50);
        assert_eq!(
            stream.needs_capture(moved, Instant::now(), false),
            Some(CaptureReason::Moved)
        );
    }

    #[test]
    fn clean_stream_does_not_capture() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = false;
        let region = physical_rect(100, 100, 200, 200);
        stream.last_region = Some(region);
        stream.last_capture = Some(Instant::now());
        // The whole point of the dirty flag: a window that repaints itself
        // every frame must not force a recapture.
        assert_eq!(stream.needs_capture(region, Instant::now(), false), None);
    }

    #[test]
    fn dirty_forces_capture() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = false;
        let region = physical_rect(0, 0, 100, 100);
        stream.last_region = Some(region);
        stream.last_capture = Some(Instant::now());
        assert_eq!(stream.needs_capture(region, Instant::now(), false), None);
        stream.dirty = true;
        assert_eq!(
            stream.needs_capture(region, Instant::now(), false),
            Some(CaptureReason::Dirty)
        );
    }

    #[test]
    fn fresh_stream_is_dirty() {
        let stream = BackdropStream::new();
        assert!(stream.dirty, "a new stream must capture right away");
        assert_eq!(
            stream.needs_capture(physical_rect(0, 0, 10, 10), Instant::now(), false),
            Some(CaptureReason::Dirty)
        );
    }

    #[test]
    fn self_heal_cadence_is_the_only_fallback() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = false;
        let region = physical_rect(0, 0, 100, 100);
        stream.last_region = Some(region);
        let start = Instant::now();
        // No capture yet: the cadence is due immediately.
        assert_eq!(stream.needs_capture(region, start, true), Some(CaptureReason::SelfHeal));
        stream.last_capture = Some(start);
        assert_eq!(stream.needs_capture(region, start, true), None);
    }

    #[test]
    fn backoff_suppresses_retries() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = true;
        let region = physical_rect(0, 0, 10, 10);
        let now = Instant::now();
        stream.retry_after = Some(now + RETRY_INTERVAL);
        assert!(stream.in_backoff(now));
        assert_eq!(stream.needs_capture(region, now, false), None);
        assert_eq!(
            stream.needs_capture(region, now + RETRY_INTERVAL, false),
            Some(CaptureReason::Dirty)
        );
    }

    /// A stream over a static desktop must stay at exactly one capture, even
    /// while the window repaints itself on every frame.
    #[test]
    fn quiet_stream_captures_once() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        let region = physical_rect(10, 10, 400, 300);
        let mut now = Instant::now();
        let mut captures = 0;
        // Ten seconds of the window animating itself over a still desktop.
        for _ in 0..600 {
            if let Some(_reason) = stream.needs_capture(region, now, false) {
                captures += 1;
                stream.last_region = Some(region);
                stream.last_capture = Some(now);
                stream.dirty = false;
            }
            now += Duration::from_millis(16);
        }
        assert_eq!(captures, 1, "static backdrop must capture exactly once");
    }

    #[test]
    fn self_heal_only_over_untracked_surfaces() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        stream.dirty = false;
        let region = physical_rect(0, 0, 100, 100);
        stream.last_region = Some(region);
        let start = Instant::now();
        // Without an untracked surface behind, the cadence never fires.
        stream.last_capture = Some(start);
        assert_eq!(
            stream.needs_capture(region, start + SELF_HEAL_INTERVAL * 10, false),
            None
        );
        // With one, it fires once the interval elapsed.
        assert_eq!(
            stream.needs_capture(region, start + SELF_HEAL_INTERVAL, true),
            Some(CaptureReason::SelfHeal)
        );
    }

    #[test]
    fn untracked_overlap_ignores_wayland_windows() {
        // A `Space<Window>` cannot be built without a display, so this only
        // pins the predicate the snapshot relies on: `toplevel()` is `Some`
        // for every Wayland window, so a Wayland-only desktop reports no
        // untracked overlap and the self-heal stays dormant.
        let space = Space::<Window>::default();
        assert!(space.elements().next().is_none());
        assert!(!untracked_overlap(
            &space,
            physical_rect(0, 0, 10, 10),
            Point::from((0, 0)),
            1.0,
        ));
    }

    #[test]
    fn physical_region_scales_and_rebases() {
        let region = logical_rect(100, 50, 200, 100);
        // Output origin at (10, 20) logical, scale 2.0.
        let phys = physical_region(region, Point::from((10, 20)), 2.0);
        assert_eq!(phys.loc, Point::from((180, 60)));
        assert_eq!(phys.size, Size::from((400, 200)));
    }

    #[test]
    fn dirty_intersecting_filters_by_overlap() {
        let mut a = BackdropStream::new();
        a.wl_surface_id = 11;
        a.last_region = Some(physical_rect(0, 0, 100, 100));
        a.dirty = false;
        let mut b = BackdropStream::new();
        b.wl_surface_id = 22;
        b.last_region = Some(physical_rect(500, 500, 100, 100));
        b.dirty = false;
        let mut streams = vec![a, b];

        // A commit inside the first stream's rect dirties that one only.
        dirty_intersecting(&mut streams.iter_mut(), physical_rect(10, 10, 10, 10), 99);
        assert!(streams[0].dirty);
        assert!(!streams[1].dirty, "a far away commit must not dirty");

        // A stream's own commit never dirties it.
        streams[0].dirty = false;
        dirty_intersecting(&mut streams.iter_mut(), physical_rect(10, 10, 10, 10), 11);
        assert!(!streams[0].dirty);
    }

    #[test]
    fn dirty_all_marks_every_stream() {
        let mut a = BackdropStream::new();
        a.dirty = false;
        let mut b = BackdropStream::new();
        b.dirty = false;
        let mut streams = vec![a, b];
        dirty_all(&mut streams.iter_mut());
        assert!(streams.iter().all(|s| s.dirty));
    }

    #[test]
    fn geometry_snapshot_detects_change() {
        let mut geometry = BackdropGeometry::new();
        // First snapshot has no predecessor, so it reports a change.
        assert!(geometry.apply(vec![(7, logical_rect(0, 0, 100, 100))]));
        // Identical layout: nothing moved.
        assert!(!geometry.apply(vec![(7, logical_rect(0, 0, 100, 100))]));
        // Same window, new position: a move.
        assert!(geometry.apply(vec![(7, logical_rect(5, 0, 100, 100))]));
        // A window appeared.
        assert!(geometry.apply(vec![
            (7, logical_rect(5, 0, 100, 100)),
            (8, logical_rect(0, 0, 50, 50)),
        ]));
        // A window disappeared: back to one entry.
        assert!(geometry.apply(vec![(7, logical_rect(5, 0, 100, 100))]));
        // A resize counts as a change too.
        assert!(geometry.apply(vec![(7, logical_rect(5, 0, 100, 120))]));
    }

    #[test]
    fn buffer_rejects_bad_geometry() {
        // Each `new` takes ownership of the descriptor, so a rejected
        // buffer closes it: use a fresh file for the second case.
        let file = test_file(64 * 64 * 4);
        let fd = file.as_raw_fd();
        std::mem::forget(file);
        assert!(BackdropBuffer::new(fd, 0, 10, 40).is_none());
        let file = test_file(64 * 64 * 4);
        let fd = file.as_raw_fd();
        std::mem::forget(file);
        assert!(BackdropBuffer::new(fd, 10, 10, 8).is_none());
    }

    #[test]
    fn buffer_writes_subsampled_rows() {
        let mut buffer = test_buffer(32);
        let mut row = vec![0u8; 16 * 4];
        for i in 0..16usize {
            row[i * 4..i * 4 + 4].copy_from_slice(&[i as u8, 2, 3, 4]);
        }
        buffer.write_row(0, 0, &row, 16, 2);
        let actual = buffer.peek(0, 0, 16, 2);
        for i in 0..16usize {
            assert_eq!(&actual[i * 4..i * 4 + 4], &[i as u8, 2, 3, 4], "pixel {i}");
        }
        // Untouched pixels stay zero.
        assert!(buffer.peek(0, 1, 16, 2).iter().all(|b| *b == 0));
    }

    #[test]
    fn buffer_writes_full_rows() {
        let mut buffer = test_buffer(32);
        let mut row = vec![0u8; 32 * 4];
        for i in 0..32usize {
            row[i * 4..i * 4 + 4].copy_from_slice(&[i as u8, 7, 8, 9]);
        }
        // step 1 is the full resolution fast path.
        buffer.write_row(3, 0, &row, 32, 1);
        let actual = buffer.peek(3, 0, 32, 1);
        assert_eq!(actual, row);
        // A neighbouring row must be untouched.
        assert!(buffer.peek(4, 0, 32, 1).iter().all(|b| *b == 0));
    }



    /// Anonymous memory file for the tests, sized to `len` bytes.
    fn test_file(len: u64) -> File {
        // SAFETY: the name is a valid NUL terminated C string and the flags
        // are valid for memfd_create.
        let fd = unsafe {
            libc::memfd_create(
                b"tontoo-backdrop-test\0".as_ptr() as *const libc::c_char,
                libc::MFD_CLOEXEC,
            )
        };
        assert!(fd >= 0, "memfd_create failed");
        // SAFETY: `fd` is a fresh descriptor that nothing else owns.
        let file = unsafe { File::from_raw_fd(fd) };
        file.set_len(len).expect("size test file");
        file
    }

    /// Mapped test buffer of `size` x `size` pixels.
    fn test_buffer(size: i32) -> BackdropBuffer {
        let file = test_file((size * size * 4) as u64);
        let fd = file.as_raw_fd();
        // Ownership of the descriptor moves into the buffer.
        std::mem::forget(file);
        BackdropBuffer::new(fd, size, size, size * 4).expect("map")
    }
}
