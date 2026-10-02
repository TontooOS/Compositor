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
    reexports::wayland_server::Resource,
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

/// Fallback cadence for backends that cannot report frame damage. The
/// client handshake already limits the rate to one frame per app frame, this
/// only keeps an animating app from recapturing at full speed.
const UNKNOWN_DAMAGE_INTERVAL: Duration = Duration::from_millis(100);

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

/// Backdrop subscription of a single `tontoo_ui_surface`.
#[derive(Debug, Clone, Default)]
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
    /// Last captured rect in logical coordinates, `None` before the first
    /// capture. A change means the window moved or was resized.
    pub last_region: Option<Rectangle<i32, Logical>>,
    /// Time of the last successful capture, used when the backend cannot
    /// report frame damage.
    pub last_capture: Option<Instant>,
}

impl BackdropStream {
    pub fn new() -> Self {
        Self {
            enabled: false,
            scale: DEFAULT_SCALE,
            ..Default::default()
        }
    }

    /// True while the compositor may try to capture this frame.
    pub fn wants_capture(&self) -> bool {
        self.enabled && self.buffer.is_some() && self.pending_ack.is_none()
    }

    /// True when a fresh frame is needed: the window moved or resized, or
    /// `damage` (output physical pixels) touched `phys_region`, or the
    /// fallback cadence elapsed and no damage is reported.
    pub fn needs_capture(
        &self,
        region: Rectangle<i32, Logical>,
        phys_region: Rectangle<i32, Physical>,
        damage: Option<&[Rectangle<i32, Physical>]>,
    ) -> bool {
        if self.last_region != Some(region) {
            return true;
        }
        match damage {
            Some(rects) => rects
                .iter()
                .any(|d| d.intersection(phys_region).is_some()),
            None => self
                .last_capture
                .map(|t| t.elapsed() >= UNKNOWN_DAMAGE_INTERVAL)
                .unwrap_or(true),
        }
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
        if y < 0 || y >= self.height || count == 0 {
            return;
        }
        let stride = self.stride as usize;
        // SAFETY: callers clamp x, y and count to the mapping bounds.
        unsafe {
            let dst = self.map.add(y as usize * stride);
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

/// Reusable offscreen render target for the readback.
pub struct BackdropCapture {
    target: Option<GlesRenderbuffer>,
}

impl Default for BackdropCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl BackdropCapture {
    pub fn new() -> Self {
        Self { target: None }
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


    // Everything strictly below the window, in compositor draw order.
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
    let below = space_elems
        .iter()
        .position(|e| target_ids.iter().any(|id| *id == e.id()))
        .unwrap_or(0);
    let below = &space_elems[..below];

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
    let mut pixels = vec![0u8; (rw as usize) * (rh as usize) * 4];
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
/// `damage` is the damage of the frame that was just presented, in output
/// physical pixels, or `None` when the backend cannot report it (udev).
pub fn update_streams(
    tontoo_ui: &mut TontooUiState,
    space: &Space<Window>,
    wallpaper: Option<(&TextureBuffer<GlesTexture>, &Wallpaper, &str, Option<f32>)>,
    reduce_transparency: bool,
    renderer: &mut GlesRenderer,
    output: &Output,
    damage: Option<&[Rectangle<i32, Physical>]>,
    scratch: &mut BackdropCapture,
    clear_color: [f32; 4],
) {
    if reduce_transparency {
        return;
    }
    if !tontoo_ui
        .surfaces()
        .any(|surface| surface.backdrop.wants_capture())
    {
        return;
    }
    let out_scale = output.current_scale().fractional_scale();
    let out_loc = space.output_geometry(output).map(|geo| geo.loc);

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
        if !surface.backdrop.needs_capture(region, phys, damage) {
            continue;
        }
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
            continue;
        };
        surface.backdrop.last_region = Some(region);
        surface.backdrop.last_capture = Some(Instant::now());
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
        let region = logical_rect(10, 10, 100, 50);
        let phys = physical_region(region, Point::from((0, 0)), 1.0);
        assert!(stream.needs_capture(region, phys, Some(&[])));
        stream.last_region = Some(region);
        assert!(!stream.needs_capture(region, phys, Some(&[])));
        let moved = logical_rect(11, 10, 100, 50);
        assert!(stream.needs_capture(moved, phys, Some(&[])));
    }

    #[test]
    fn damage_inside_rect_forces_capture() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        let region = logical_rect(100, 100, 200, 200);
        stream.last_region = Some(region);
        let phys = physical_region(region, Point::from((0, 0)), 1.0);
        let inside = [physical_rect(150, 150, 10, 10)];
        assert!(stream.needs_capture(region, phys, Some(&inside)));
        let outside = [physical_rect(0, 0, 10, 10)];
        assert!(!stream.needs_capture(region, phys, Some(&outside)));
    }

    #[test]
    fn unknown_damage_uses_cadence() {
        let mut stream = BackdropStream::new();
        stream.enabled = true;
        let region = logical_rect(0, 0, 100, 100);
        let phys = physical_region(region, Point::from((0, 0)), 1.0);
        stream.last_region = Some(region);
        assert!(stream.needs_capture(region, phys, None));
        stream.last_capture = Some(Instant::now());
        assert!(!stream.needs_capture(region, phys, None));
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
