use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use smithay::{
    backend::{
        allocator::{
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
            Format, Fourcc,
        },
        drm::exporter::gbm::GbmFramebufferExporter,
        drm::{
            compositor::{DrmCompositor, FrameFlags, PrimaryPlaneElement},
            DrmDevice, DrmDeviceFd, DrmEvent, DrmNode,
        },
        egl::{EGLContext, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            element::{
                texture::{TextureBuffer, TextureRenderElement},
                Kind,
            },
            gles::{GlesRenderer, GlesTexture},
        },
        session::{libseat::LibSeatSession, Session},
        udev::{UdevBackend, UdevEvent},
        SwapBuffersError,
    },
    desktop::{layer_map_for_output, space::space_render_elements, utils::OutputPresentationFeedback, Window},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{EventLoop, RegistrationToken},
        drm::{
            self as drm_crate,
            control::{connector, crtc, Device as _, ModeTypeFlags},
        },
        input::Libinput,
        wayland_server::DisplayHandle,
    },
    utils::{Clock, DeviceFd, IsAlive, Monotonic, Physical, Point, Rectangle, Size, Transform},
};

use crate::cursor::{
    CursorRenderElement, CursorTextureElement, DockBarElement,
    TontooRenderElements, WallpaperElement, WindowBorderElement,
    WindowShadowElement,
};
use crate::{wallpaper::Wallpaper, TontooCompositor};

type TontooDrmCompositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    Option<OutputPresentationFeedback>,
    DrmDeviceFd,
>;

pub struct UdevData {
    pub session: LibSeatSession,
    pub devices: HashMap<DrmNode, DeviceData>,
}

pub struct DeviceData {
    pub drm: DrmDevice,
    pub drm_fd: DrmDeviceFd,
    pub gbm: GbmDevice<DrmDeviceFd>,
    pub gles: GlesRenderer,
    pub renderer_formats: Vec<Format>,
    pub surfaces: HashMap<crtc::Handle, SurfaceData>,
    pub known_connectors: HashMap<connector::Handle, crtc::Handle>,
    pub render_node: DrmNode,
    pub registration_token: Option<RegistrationToken>,
}

pub struct SurfaceData {
    pub dh: DisplayHandle,
    pub compositor: TontooDrmCompositor,
    pub output: Output,
}

pub fn init_udev(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
) -> Result<(), Box<dyn std::error::Error>> {
    let (session, notifier) = LibSeatSession::new().map_err(|e| {
        tracing::error!("Failed to create libseat session: {}", e);
        e
    })?;

    event_loop
        .handle()
        .insert_source(notifier, |event, _, state| {
            match event {
                smithay::backend::session::Event::ActivateSession => {
                    tracing::info!("Session activated (VT switch back)");
                    crate::udev::try_render_all(state);
                }
                smithay::backend::session::Event::PauseSession => {
                    tracing::info!("Session paused (VT switch away)");
                }
            }
        })?;

    let seat = session.seat();
    tracing::info!("Libseat session created for seat: {}", seat);

    let mut libinput_context = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
        LibinputSessionInterface::from(session.clone()),
    );
    libinput_context.udev_assign_seat(&seat).unwrap();
    let libinput_backend = LibinputInputBackend::new(libinput_context.clone());

    event_loop
        .handle()
        .insert_source(libinput_backend, move |event, _, state| {
            let is_keyboard = matches!(event, smithay::backend::input::InputEvent::Keyboard { .. });
            state.process_input_event(event);
            // For keyboard, trigger an immediate render on a separate thread
            // so the main thread's libinput is never blocked by `commit_frame`
            // (which on llvmpipe can take 10-20ms). This is the first step
            // towards a full RenderThread like Mutter/KWin.
            if is_keyboard {
                state.request_redraw();
                state.loop_signal.wakeup();
                // Also try to render immediately on the next event loop iteration
                // without waiting for the 16ms timer - the timer will also pick it up.
            } else {
                state.request_redraw();
            }
            // Flush server-to-client events (key/button/motion) immediately.
            // Without this, queued events sit in userspace buffers until
            // unrelated client traffic triggers a flush, which stalls typing
            // in clients like foot for tens of seconds. Same pattern as the
            // winit backend (render.rs) after sending frame callbacks.
            let _ = state.display_handle.flush_clients();
        })?;

    let backend = UdevBackend::new(&seat).map_err(|e| {
        tracing::error!("Failed to create udev backend: {}", e);
        e
    })?;

    state.udev_data = Some(UdevData {
        session,
        devices: HashMap::new(),
    });

    let nodes: Vec<_> = backend
        .device_list()
        .map(|(id, p)| (id, p.to_owned()))
        .collect();
    for (id, path) in nodes {
        if let Err(e) = add_node(event_loop, state, id, path) {
            tracing::error!("Failed to add device: {}", e);
        }
    }

    event_loop
        .handle()
        .insert_source(backend, move |event, _, state| match event {
            UdevEvent::Added {
                device_id: _,
                path: _,
            } => {
                tracing::info!("New DRM device detected (hot-plug not yet supported)");
            }
            UdevEvent::Changed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    if let Err(e) = scan_connectors(state, node) {
                        tracing::error!("Failed to rescan connectors: {}", e);
                    }
                }
            }
            UdevEvent::Removed { device_id } => {
                if let Ok(node) = DrmNode::from_dev_id(device_id) {
                    if let Some(udev) = state.udev_data.as_mut() {
                        if let Some(device) = udev.devices.remove(&node) {
                            for (_, surface) in device.surfaces {
                                state.space.unmap_output(&surface.output);
                            }
                            tracing::info!("Removed DRM device: {:?}", node);
                        }
                    }
                }
            }
        })?;

    try_render_all(state);

    // Render pump: vmwgfx and other virtualized drivers do not deliver reliable
    // page-flip completion (VBlank) events. Frames are therefore presented with
    // `commit_frame` (synchronous, no VBlank required) and this timer drives
    // rendering while something actually needs frames (input damage,
    // Wayland commits, dock animations).
    //
    // IMPORTANT: this timer must NOT call `try_render_all` unconditionally.
    // smithay's DrmCompositor treats every `render_frame` as a real frame (the
    // primary plane is never skipped), so each call performs an atomic DRM
    // commit even for a pixel-identical desktop. On VirtualBox a permanent
    // commit stream saturates the virtual GPU and starves the whole guest.
    //
    // The interval is 33 ms (~30 fps) instead of 16 ms: vboxvideo has no
    // separate cursor plane, so every cursor movement triggers a full atomic
    // commit. Each commit briefly blanks the scanout buffer on VBoxSVGA,
    // which is visible as flickering. Halving the commit rate makes this
    // barely noticeable while the cursor stays smooth enough.
    // For keyboard responsiveness we use 16ms (60fps) instead of 33ms - the
    // libinput error "event processing lagging behind by 22ms" showed the
    // previous 33ms pump was too coarse for VirtualBox's 22ms lag warning.
    event_loop
        .handle()
        .insert_source(
            smithay::reexports::calloop::timer::Timer::from_duration(
                std::time::Duration::from_millis(16),
            ),
            |_, _, state| {
                // Ghost-shadow fix: detect dead windows even when idle and force a redraw.
                // `Space::refresh` is also called inside `try_render_all`, but the timer
                // must trigger it even when `pending_redraw` is false.
                {
                    use smithay::utils::IsAlive;
                    if state.space.elements().any(|w| !w.alive()) {
                        state.pending_redraw = true;
                    }
                }
                if state.pending_redraw
                    || state.shell.dock.is_animating()
                    || state.animation_manager.has_active()
                {
                    crate::udev::try_render_all(state);
                } else {
                    // Safety: if dead windows appeared after the check above (race), force one frame
                    use smithay::utils::IsAlive;
                    if state.space.elements().any(|w| !w.alive()) {
                        crate::udev::try_render_all(state);
                    }
                }
                // Flush frame callbacks so clients redraw immediately instead
                // of waiting for unrelated client traffic (see libinput flush).
                let _ = state.display_handle.flush_clients();
                smithay::reexports::calloop::timer::TimeoutAction::ToDuration(
                    std::time::Duration::from_millis(16),
                )
            },
        )
        .map_err(|e| -> Box<dyn std::error::Error> {
            tracing::error!("Failed to insert render timer: {}", e);
            Box::new(e)
        })?;

    tracing::info!("Udev backend initialized");
    Ok(())
}

fn add_node(
    event_loop: &mut EventLoop<TontooCompositor>,
    state: &mut TontooCompositor,
    device_id: libc::dev_t,
    path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let node = DrmNode::from_dev_id(device_id)?;

    // Open the DRM device through the libseat session instead of directly:
    // seatd opens the device as root and passes the fd back, so the compositor
    // works as an unprivileged user even when /dev/dri/* is root-only.
    let session = &mut state
        .udev_data
        .as_mut()
        .ok_or("udev data missing")?
        .session;
    let owned_fd = session
        .open(&path, smithay::reexports::rustix::fs::OFlags::RDWR)
        .map_err(|e| {
            tracing::error!("Failed to open DRM device {:?} via session: {:?}", path, e);
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{:?}", e))
        })?;
    let device_fd = DeviceFd::from(owned_fd);

    let drm_fd = DrmDeviceFd::new(device_fd);
    let (drm, notifier) = DrmDevice::new(drm_fd.clone(), false)?;
    let gbm = GbmDevice::new(drm_fd.clone())?;
    let egl_display = unsafe { EGLDisplay::new(gbm.clone())? };
    let egl_context = EGLContext::new(&egl_display)?;
    let gles = unsafe { GlesRenderer::new(egl_context)? };
    let renderer_formats = egl_display
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect::<Vec<_>>();

    let udev = state.udev_data.as_mut().ok_or("udev data missing")?;
    udev.devices.insert(
        node,
        DeviceData {
            drm,
            drm_fd,
            gbm,
            gles,
            renderer_formats,
            surfaces: HashMap::new(),
            known_connectors: HashMap::new(),
            render_node: node,
            registration_token: None,
        },
    );

    if let Err(e) = scan_connectors(state, node) {
        tracing::error!("Failed to scan connectors: {}", e);
    }

    let token = event_loop
        .handle()
        .insert_source(notifier, move |event, _, state| {
            match event {
                DrmEvent::VBlank(crtc) => {
                    // Frames are presented with `commit_frame`, which does not
                    // generate page-flip events. When a driver does deliver them
                    // anyway, just kick a re-render.
                    tracing::trace!("VBlank on crtc {:?}", crtc);
                    try_render_all(state);
                }
                DrmEvent::Error(e) => {
                    tracing::error!("DRM error event: {:?}", e);
                }
            }
        })?;
    state
        .udev_data
        .as_mut()
        .unwrap()
        .devices
        .get_mut(&node)
        .unwrap()
        .registration_token = Some(token);

    tracing::info!("Added DRM device: {:?}", node);
    Ok(())
}

/// Pick the native mode for a connector: the EDID PREFERRED mode.
/// If several modes carry PREFERRED, take the one with the highest refresh;
/// on ties take the smaller area (avoids 4K duplicates on VMs).
/// Fallback is the first advertised mode. Returns None when empty.
fn pick_connector_mode(
    modes: &[drm_crate::control::Mode],
) -> Option<drm_crate::control::Mode> {
    let mut preferred: Vec<&drm_crate::control::Mode> = modes
        .iter()
        .filter(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
        .collect();
    if !preferred.is_empty() {
        preferred.sort_by_key(|m| {
            let (w, h) = m.size();
            // Highest refresh first, then smaller area.
            (std::cmp::Reverse(m.vrefresh()), w as u32 * h as u32)
        });
        return preferred.into_iter().next().copied();
    }
    modes.first().copied()
}

fn log_connector_modes(connector: &connector::Info) {
    for m in connector.modes() {
        tracing::info!(
            "Connector mode: {}x{} @ {}Hz{}",
            m.size().0,
            m.size().1,
            m.vrefresh(),
            if m.mode_type().contains(ModeTypeFlags::PREFERRED) {
                " (PREFERRED)"
            } else {
                ""
            }
        );
    }
}

fn scan_connectors(
    state: &mut TontooCompositor,
    node: DrmNode,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = state
        .udev_data
        .as_mut()
        .ok_or("udev data missing")?
        .devices
        .get_mut(&node)
        .ok_or("device missing")?;

    let resource_handles = device.drm.resource_handles().map_err(|e| {
        tracing::error!("Failed to get DRM resource handles: {}", e);
        e
    })?;

    let used_crtcs: HashSet<crtc::Handle> = device.surfaces.keys().copied().collect();

    let mut still_connected = HashSet::new();

    for &conn_handle in resource_handles.connectors() {
        let conn = match device.drm.get_connector(conn_handle, false) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to get connector {:?}: {}", conn_handle, e);
                continue;
            }
        };

        let is_connected = conn.state() == connector::State::Connected;

        if is_connected {
            still_connected.insert(conn_handle);

            if device.known_connectors.contains_key(&conn_handle) {
                continue;
            }

            let crtc = match find_crtc(&device.drm, &conn, &resource_handles, &used_crtcs) {
                Some(c) => c,
                None => {
                    tracing::warn!("No available CRTC for connector {:?}", conn_handle);
                    continue;
                }
            };

            let output = match create_output_for_connector(&state.display_handle, &conn) {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!(
                        "Failed to create output for connector {:?}: {}",
                        conn_handle,
                        e
                    );
                    continue;
                }
            };

            log_connector_modes(&conn);

            // Use the native screen mode (EDID PREFERRED) so the framebuffer
            // matches the display. Never pick the largest mode: VMs advertise
            // huge 4K+ modes that do not fit the screen.
            let preferred_mode = pick_connector_mode(conn.modes()).ok_or("No modes available")?;
            let drm_mode = preferred_mode;
            let wl_mode = Mode::from(drm_mode);
            tracing::info!(
                "Chosen mode for connector {:?}: {}x{} @ {}Hz",
                conn_handle,
                drm_mode.size().0,
                drm_mode.size().1,
                drm_mode.vrefresh()
            );

            let x = state.space.outputs().fold(0, |acc, o| {
                acc + state
                    .space
                    .output_geometry(o)
                    .map(|g| g.size.w)
                    .unwrap_or(0)
            });
            let position = (x, 0).into();

            output.change_current_state(
                Some(wl_mode),
                Some(Transform::Normal),
                None,
                Some(position),
            );
            state.space.map_output(&output, position);

            let surface = device
                .drm
                .create_surface(crtc, drm_mode, &[conn.handle()])?;
            let allocator = GbmAllocator::new(
                device.gbm.clone(),
                GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
            );
            let exporter = GbmFramebufferExporter::new(device.gbm.clone(), None);
            let compositor = DrmCompositor::new(
                &output,
                surface,
                None,
                allocator,
                exporter,
                vec![Fourcc::Xrgb8888],
                device.renderer_formats.clone(),
                device.drm.cursor_size(),
                Some(device.gbm.clone()),
            )?;

            device.surfaces.insert(
                crtc,
                SurfaceData {
                    dh: state.display_handle.clone(),
                    compositor,
                    output: output.clone(),
                },
            );
            device.known_connectors.insert(conn_handle, crtc);
            tracing::info!("Connected: {} on CRTC {:?}", output.name(), crtc);
        }
    }

    let disconnected: Vec<_> = device
        .known_connectors
        .iter()
        .filter(|(h, _)| !still_connected.contains(h))
        .map(|(&h, &c)| (h, c))
        .collect();

    for (conn_handle, crtc) in disconnected {
        device.known_connectors.remove(&conn_handle);
        if let Some(surface) = device.surfaces.remove(&crtc) {
            state.space.unmap_output(&surface.output);
            tracing::info!("Disconnected connector {:?} / CRTC {:?}", conn_handle, crtc);
        }
    }

    Ok(())
}

fn find_crtc(
    drm: &DrmDevice,
    connector: &connector::Info,
    resource_handles: &drm_crate::control::ResourceHandles,
    used_crtcs: &HashSet<crtc::Handle>,
) -> Option<crtc::Handle> {
    for encoder_handle in connector.encoders() {
        let encoder = match drm.get_encoder(*encoder_handle) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let possible = resource_handles.filter_crtcs(encoder.possible_crtcs());
        for crtc_handle in possible {
            if !used_crtcs.contains(&crtc_handle) {
                return Some(crtc_handle);
            }
        }
    }
    None
}

fn create_output_for_connector(
    display_handle: &DisplayHandle,
    connector: &connector::Info,
) -> Result<Output, Box<dyn std::error::Error>> {
    let name = format!(
        "{}-{}",
        connector.interface().as_str(),
        connector.interface_id()
    );

    let (phys_w, phys_h) = connector.size().unwrap_or((0, 0));
    let output = Output::new(
        name,
        PhysicalProperties {
            size: (phys_w as i32, phys_h as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "Unknown".into(),
            model: "Unknown".into(),
        },
    );

    let preferred_mode =
        pick_connector_mode(connector.modes()).ok_or("No modes available")?;

    let wl_mode = Mode::from(preferred_mode);

    let _global = output.create_global::<TontooCompositor>(display_handle);
    output.set_preferred(wl_mode);

    tracing::info!(
        "Created output: {} ({}x{} @ {}Hz)",
        output.name(),
        preferred_mode.size().0,
        preferred_mode.size().1,
        preferred_mode.vrefresh()
    );

    Ok(output)
}

/// Render all outputs immediately (event-driven render pump).
pub fn try_render_all(state: &mut TontooCompositor) {
    // Fix ghost shadows: clean up destroyed windows before rendering.
    // `Space::refresh()` removes windows whose `wl_surface` is no longer alive.
    // Without this, `space.elements()` keeps dead windows and their shadows are
    // re-rendered every frame, leaving ghost artifacts after close.
    // Also clean up stale popups.
    {
        use smithay::utils::IsAlive;
        let before = state.space.elements().count();
        state.space.refresh();
        state.popups.cleanup();
        if state.space.elements().count() != before {
            tracing::debug!("space.refresh: removed {} dead window(s), forcing redraw", before - state.space.elements().count());
            state.pending_redraw = true;
        }
        // Also check for any remaining dead surfaces that are alive==false but not yet removed
        // (paranoia: ensure they don't contribute shadows)
        if state.space.elements().any(|w| !w.alive()) {
            state.pending_redraw = true;
        }
    }

    // Dock animation: advance spring physics with the real elapsed time so
    // event-driven renders (which may be far apart) stay smooth.
    let dt = state.last_render.elapsed().as_secs_f32().min(0.1);
    state.last_render = std::time::Instant::now();
    state.shell.dock.tick(dt);

    // The frame about to be rendered satisfies every pending damage request;
    // the render pump re-sets this when new input or Wayland commits arrive.
    state.pending_redraw = false;

    // Compute dock magnification based on pointer position on first output
    let pointer_x = state.seat.get_pointer().map(|p| p.current_location().x);
    if let Some(x) = pointer_x {
        if let Some(output) = state.space.outputs().next() {
            if let Some(geo) = state.space.output_geometry(output) {
                state
                    .shell
                    .dock
                    .compute_magnification(x as f32, geo.size.w as f32);
            }
        }
    }

    // Split field borrows to avoid conflicts when passing multiple refs
    let space = &state.space;
    let cursor = &mut state.cursor;
    let wallpaper = state.wallpaper.as_ref();
    let wallpaper_buffer = &mut state.wallpaper_buffer;
    let seat = &state.seat;
    let active_app = &state.shell.dock.active_app;
    let render_cache = &mut state.render_cache;
    let tontoo_ui = &state.tontoo_ui;

    let Some(udev) = state.udev_data.as_mut() else {
        return;
    };

    let clear_color = state.color_scheme.clear_color();
    let pointer_pos = seat.get_pointer().map(|p| p.current_location());

    for device in udev.devices.values_mut() {
        let DeviceData { gles, surfaces, .. } = device;
        for surface in surfaces.values_mut() {
            if let Err(e) = render_surface(
                surface,
                gles,
                space,
                clear_color,
                wallpaper,
                Some(cursor),
                pointer_pos,
                wallpaper_buffer,
                active_app,
                render_cache,
                tontoo_ui,
                state.focused_surface.as_ref(),
                &state.shell.window_controls,
            ) {
                tracing::error!("render_surface failed: {:?}", e);
            }
        }
    }
}

/// Upload wallpaper pixel data to GPU and return a cached buffer.
/// Called once, then the buffer is reused across frames via `wallpaper_buffer_to_element`.
fn create_wallpaper_buffer(
    renderer: &mut GlesRenderer,
    wallpaper: &Wallpaper,
) -> Option<TextureBuffer<GlesTexture>> {
    match TextureBuffer::from_memory(
        renderer,
        wallpaper.pixels(),
        Fourcc::Abgr8888,
        wallpaper.size(),
        false,
        1,
        Transform::Normal,
        None,
    ) {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::error!(
                "failed to upload wallpaper texture {}x{}: {}",
                wallpaper.size().0,
                wallpaper.size().1,
                e
            );
            None
        }
    }
}

/// Create a lightweight wallpaper render element from a cached GPU buffer.
fn wallpaper_buffer_to_element(
    buffer: &TextureBuffer<GlesTexture>,
    wallpaper: &Wallpaper,
    output_size: Size<i32, Physical>,
) -> TextureRenderElement<GlesTexture> {
    let (wp_w, wp_h) = wallpaper.size();
    let scale_x = output_size.w as f64 / wp_w as f64;
    let scale_y = output_size.h as f64 / wp_h as f64;
    let fill_scale = scale_x.max(scale_y);

    let scaled_w = (wp_w as f64 * fill_scale) as i32;
    let scaled_h = (wp_h as f64 * fill_scale) as i32;
    let offset_x = ((output_size.w - scaled_w) / 2) as f64;
    let offset_y = ((output_size.h - scaled_h) / 2) as f64;

    let src = Rectangle::from_size(Size::from((wp_w as f64, wp_h as f64)));

    TextureRenderElement::from_texture_buffer(
        Point::from((offset_x, offset_y)),
        buffer,
        None,
        Some(src),
        Some(Size::from((scaled_w, scaled_h))),
        Kind::Unspecified,
    )
}

/// Create a glass-effect texture buffer with 2-pass software blur, adaptive wallpaper color, and 1px border.
/// `blur_src` = (wallpaper_pixels, wp_w, wp_h, screen_w, screen_h, dock_x, dock_y)
fn create_glass_texture(
    renderer: &mut GlesRenderer,
    w: i32,
    h: i32,
    cr: i32,
    blur_src: Option<(&[u8], i32, i32, i32, i32, i32, i32)>,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>> {
    let wu = w as u32;
    let hu = h as u32;
    let mut data = vec![0u8; (wu * hu * 4) as usize];

    if let Some((wp_dat, wp_w, wp_h, sc_w, sc_h, dx, dy)) = blur_src {
        let fill_scale = (sc_w as f64 / wp_w as f64).max(sc_h as f64 / wp_h as f64);
        let off_x = ((sc_w as f64 - wp_w as f64 * fill_scale) / 2.0) as f64;
        let off_y = ((sc_h as f64 - wp_h as f64 * fill_scale) / 2.0) as f64;

        // 2-pass box blur (~10x10 Gaussian)
        // Map each texture pixel to the wallpaper: screen px = region origin + tex px / tex_scale,
        // wallpaper px = (screen px - wallpaper fill offset) / fill_scale.
        let mut tmp = vec![0u8; (wu * hu * 4) as usize];
        let tex_to_screen = 1.0 / tex_scale as f64;
        box_blur_5x5(
            wp_dat, &mut tmp, wu, hu, wp_w as u32, wp_h as u32,
            (dx as f64 - off_x) as i32, (dy as f64 - off_y) as i32,
            tex_to_screen, tex_to_screen, fill_scale,
        );
        box_blur_5x5(&tmp, &mut data, wu, hu, wu, hu, 0, 0, 0.0, 0.0, 1.0);

        // Glass overlay: 30% white tint over blurred wallpaper (matches vorschau alpha=0.32)
        for i in (0..data.len()).step_by(4) {
            data[i]     = (data[i] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i + 1] = (data[i + 1] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i + 2] = (data[i + 2] as f32 * 0.70 + 255.0 * 0.30) as u8;
            data[i + 3] = 255;
        }
    } else {
        for i in (0..data.len()).step_by(4) {
            data[i] = 255; data[i + 1] = 255; data[i + 2] = 255; data[i + 3] = 77;
        }
    }

    // Anti-aliased rounded corners + 2px border (opacity 0.63 ≈ 160)
    for y in 0..hu { for x in 0..wu {
        let dist = signed_dist_rounded(x as f64, y as f64, wu as f64, hu as f64, cr as f64);
        let i = ((y * wu + x) * 4) as usize;

        if dist < 0.0 {
            // Inside rounded rect → keep glass content, full opacity
            data[i + 3] = 255;
        } else if dist < 4.0 {
            // Border zone: 2px white border (4 texture px at 2× scale), fade out
            let border_t = dist; // 0..4
            let border_alpha = (160.0 * (1.0 - border_t / 4.0)) as u8;
            let bg_r = data[i] as f32; let bg_g = data[i + 1] as f32; let bg_b = data[i + 2] as f32;
            let t = border_alpha as f32 / 255.0;
            data[i]     = (bg_r * (1.0 - t) + 255.0 * t) as u8;
            data[i + 1] = (bg_g * (1.0 - t) + 255.0 * t) as u8;
            data[i + 2] = (bg_b * (1.0 - t) + 255.0 * t) as u8;
            data[i + 3] = 255;
        } else {
            // Outside → transparent
            data[i] = 0; data[i + 1] = 0; data[i + 2] = 0; data[i + 3] = 0;
        }
    }}

    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (w, h), false, tex_scale, Transform::Normal, None).ok()
}

/// 5×5 box blur kernel. Blurs source pixels into dest.
fn box_blur_5x5(
    src: &[u8], dest: &mut [u8],
    dw: u32, dh: u32,
    sw: u32, sh: u32,
    off_x: i32, off_y: i32,
    sx: f64, sy: f64,
    fill_scale: f64,
) {
    let kernel_size: i32 = 2;
    for oy in 0..dh {
        for ox in 0..dw {
            // Map dest pixel to source pixel (if source is wallpaper-sized)
            let spx = if sx > 0.0 { ((ox as f64 * sx + off_x as f64) / fill_scale) as i32 } else { ox as i32 };
            let spy = if sy > 0.0 { ((oy as f64 * sy + off_y as f64) / fill_scale) as i32 } else { oy as i32 };

            let mut r = 0u32; let mut g = 0u32; let mut b = 0u32; let mut a = 0u32; let mut cnt = 0u32;
            for ky in -kernel_size..=kernel_size {
                for kx in -kernel_size..=kernel_size {
                    let px = (spx + kx).clamp(0, sw as i32 - 1) as u32;
                    let py = (spy + ky).clamp(0, sh as i32 - 1) as u32;
                    let idx = ((py * sw + px) * 4) as usize;
                    if idx + 3 < src.len() {
                        r += src[idx] as u32;
                        g += src[idx + 1] as u32;
                        b += src[idx + 2] as u32;
                        a += src[idx + 3] as u32;
                        cnt += 1;
                    }
                }
            }
            let di = ((oy * dw + ox) * 4) as usize;
            if cnt > 0 { r /= cnt; g /= cnt; b /= cnt; a /= cnt; }
            dest[di] = r as u8; dest[di + 1] = g as u8; dest[di + 2] = b as u8; dest[di + 3] = a as u8;
        }
    }
}

/// Signed distance to rounded rectangle (negative = inside, positive = outside).
fn signed_dist_rounded(x: f64, y: f64, w: f64, h: f64, r: f64) -> f64 {
    let dx = x.max(r).min(w - r) - x;
    let dy = y.max(r).min(h - r) - y;
    (dx * dx + dy * dy).sqrt() - r
}

// ── Window decoration constants (macOS Tahoe 1:1) ──
// CSD: compositor only draws shadow + border; apps draw their own header.

const WINDOW_CORNER_RADIUS: f64 = 10.0;
const WINDOW_SHADOW_OFFSET_Y: f64 = 8.0;
const WINDOW_SHADOW_BLUR: f64 = 40.0;
const WINDOW_SHADOW_BASE_ALPHA_DARK: f64 = 0.22;
const WINDOW_SHADOW_BASE_ALPHA_LIGHT: f64 = 0.13;
const WINDOW_BORDER_WIDTH: f64 = 0.7;

/// Create a high-quality window shadow texture — 3-layer Gaussian model
/// (tight/medium/far) with vertical bias for realistic macOS-style drop shadow.
fn create_window_shadow_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    win_h: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let cr = WINDOW_CORNER_RADIUS;
    let pad: i32 = 40;
    let tex_w = win_w + pad * 2;
    let tex_h = win_h + pad * 2;
    if tex_w <= 0 || tex_h <= 0 {
        return None;
    }
    let tw = tex_w as u32;
    let th = tex_h as u32;
    let mut data = vec![0u8; (tw * th * 4) as usize];

    let base_alpha = match color_scheme {
        crate::config::ColorScheme::Dark => WINDOW_SHADOW_BASE_ALPHA_DARK,
        crate::config::ColorScheme::Light => WINDOW_SHADOW_BASE_ALPHA_LIGHT,
    };

    let win_wf = win_w as f64;
    let win_hf = win_h as f64;
    let pad_f = pad as f64;

    for y in 0..th {
        for x in 0..tw {
            let wx = x as f64 - pad_f;
            let wy = y as f64 - pad_f;
            let dist = signed_dist_rounded(wx, wy, win_wf, win_hf, cr);

            let shadow_alpha = if dist > 0.0 {
                let tight = (-0.5 * (dist / 10.0).powi(2)).exp();
                let medium = (-0.5 * (dist / 22.0).powi(2)).exp();
                let far = (-0.5 * (dist / 40.0).powi(2)).exp();
                let intensity = 0.30 * tight + 0.30 * medium + 0.40 * far;
                // Smooth fade to transparent at texture edge (outer 12px) to avoid hard cutoff
                let edge_fade = ((pad_f - dist) / 12.0).clamp(0.0, 1.0);
                let center_y = pad_f + win_hf / 2.0;
                let dy_center = y as f64 - center_y;
                let bias_norm = (dy_center / (win_hf / 2.0 + pad_f)).clamp(-1.0, 1.0);
                let bias = bias_norm * 0.15;
                ((intensity * edge_fade * base_alpha * (1.0 + bias) * 255.0).clamp(0.0, 255.0)) as u8
            } else {
                0
            };

            let i = ((y * tw + x) * 4) as usize;
            data[i] = 0;
            data[i + 1] = 0;
            data[i + 2] = 0;
            data[i + 3] = shadow_alpha;
        }
    }

    TextureBuffer::from_memory(
        renderer, &data, Fourcc::Abgr8888, (tex_w, tex_h),
        false, 1, Transform::Normal, None,
    ).ok()
}

/// Create a window border + rounded-corner mask texture with proper anti-aliasing.
/// Outside the rounded rect is filled with the desktop clear color (matching the
/// wallpaper fallback), and the rounded edge has 1-2px feather to avoid pixely corners.
fn create_window_border_mask_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    win_h: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let cr = WINDOW_CORNER_RADIUS;
    let bw = WINDOW_BORDER_WIDTH;
    let tw = win_w as u32;
    let th = win_h as u32;
    if tw == 0 || th == 0 {
        return None;
    }
    let mut data = vec![0u8; (tw * th * 4) as usize];

    let (bg_r, bg_g, bg_b) = match color_scheme {
        crate::config::ColorScheme::Dark => (28u8, 28u8, 28u8),
        crate::config::ColorScheme::Light => (236u8, 236u8, 236u8),
    };
    let (bd_r, bd_g, bd_b, bd_a) = match color_scheme {
        crate::config::ColorScheme::Dark => (0u8, 0u8, 0u8, 60u8),
        crate::config::ColorScheme::Light => (0u8, 0u8, 0u8, 25u8),
    };

    for y in 0..th {
        for x in 0..tw {
            let dist = signed_dist_rounded(x as f64, y as f64, tw as f64, th as f64, cr);
            let i = ((y * tw + x) * 4) as usize;

            if dist > 1.0 {
                data[i] = bg_r;
                data[i + 1] = bg_g;
                data[i + 2] = bg_b;
                data[i + 3] = 255;
            } else if dist > 0.0 {
                let aa = (1.0 - dist).clamp(0.0, 1.0) as f32;
                data[i] = (bg_r as f32 * (1.0f32 - aa * 0.5f32) + bd_r as f32 * aa * 0.5) as u8;
                data[i + 1] = (bg_g as f32 * (1.0f32 - aa * 0.5f32) + bd_g as f32 * aa * 0.5) as u8;
                data[i + 2] = (bg_b as f32 * (1.0f32 - aa * 0.5f32) + bd_b as f32 * aa * 0.5) as u8;
                data[i + 3] = (255.0 * (1.0f32 - aa * 0.5f32) + bd_a as f32 * aa * 0.5) as u8;
            } else if dist > -bw {
                let edge_alpha = ((-dist) / bw).min(1.0);
                let aa = if dist > -0.5 { 1.0 - (-dist - 0.5).abs() * 2.0 } else { 1.0 };
                let aa = aa.clamp(0.0, 1.0);
                data[i] = bd_r;
                data[i + 1] = bd_g;
                data[i + 2] = bd_b;
                data[i + 3] = (bd_a as f64 * edge_alpha * aa) as u8;
            } else if dist > -bw - 1.0 {
                let aa = (dist + bw + 1.0).clamp(0.0, 1.0);
                data[i] = bd_r;
                data[i + 1] = bd_g;
                data[i + 2] = bd_b;
                data[i + 3] = (bd_a as f64 * aa * 0.5) as u8;
            } else {
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 0;
            }
        }
    }

    TextureBuffer::from_memory(
        renderer, &data, Fourcc::Abgr8888, (win_w, win_h),
        false, 1, Transform::Normal, None,
    ).ok()
}

fn create_window_titlebar_texture(
    renderer: &mut GlesRenderer,
    win_w: i32,
    color_scheme: crate::config::ColorScheme,
) -> Option<TextureBuffer<GlesTexture>> {
    let tw = win_w as u32;
    let th = crate::config::TITLEBAR_HEIGHT as u32;
    if tw == 0 {
        return None;
    }
    let mut data = vec![0u8; (tw * th * 4) as usize];

    // Glass / translucent titlebar with rounded top corners
    let corner_r: f32 = 10.0;
    let milkiness: f32 = 0.65;
    let (bg_r, bg_g, bg_b) = match color_scheme {
        crate::config::ColorScheme::Dark => (29u8, 29u8, 29u8),
        crate::config::ColorScheme::Light => (236u8, 236u8, 236u8),
    };

    for y in 0..th {
        for x in 0..tw {
            let i = ((y * tw + x) * 4) as usize;
            // Rounded top corners
            let alpha = if (x as f32) < corner_r && (y as f32) < corner_r {
                let dx = corner_r - x as f32;
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r { 0.0 } else { 1.0 - (1.0 - dist / corner_r).powf(1.5) }
            } else if (x as f32) >= tw as f32 - corner_r && (y as f32) < corner_r {
                let dx = x as f32 - (tw as f32 - corner_r);
                let dy = corner_r - y as f32;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > corner_r { 0.0 } else { 1.0 - (1.0 - dist / corner_r).powf(1.5) }
            } else {
                1.0
            };
            let a = (alpha * 180.0 * milkiness) as u8; // translucent
            data[i]     = bg_b;
            data[i + 1] = bg_g;
            data[i + 2] = bg_r;
            data[i + 3] = a;
        }
    }

    TextureBuffer::from_memory(
        renderer,
        &data,
        Fourcc::Abgr8888,
        (win_w, crate::config::TITLEBAR_HEIGHT),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()
}

/// Derive ColorScheme from clear_color (reverse of ColorScheme::clear_color).
fn clear_color_to_scheme(clear_color: [f32; 4]) -> crate::config::ColorScheme {
    // Dark clear_color = [0.11, 0.11, 0.11, 1.0], Light = [0.93, 0.93, 0.93, 1.0]
    if clear_color[0] < 0.5 {
        crate::config::ColorScheme::Dark
    } else {
        crate::config::ColorScheme::Light
    }
}

/// Draw a 5×5 letter pattern into a pixel buffer.
fn draw_letter_bitmap(
    data: &mut [u8],
    buf_w: u32,
    x: i32,
    y: i32,
    letter: char,
    color: u32,
) {
    let pattern: Vec<Vec<u8>> = match letter {
        'T' => vec![
            vec![1, 1, 1, 1, 1],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 1, 0, 0],
            vec![0, 0, 1, 0, 0],
        ],
        'N' => vec![
            vec![1, 0, 0, 0, 1],
            vec![1, 1, 0, 0, 1],
            vec![1, 0, 1, 0, 1],
            vec![1, 0, 0, 1, 1],
            vec![1, 0, 0, 0, 1],
        ],
        'F' => vec![
            vec![1, 1, 1, 1, 1],
            vec![1, 0, 0, 0, 0],
            vec![1, 1, 1, 0, 0],
            vec![1, 0, 0, 0, 0],
            vec![1, 0, 0, 0, 0],
        ],
        'S' => vec![
            vec![0, 1, 1, 1, 0],
            vec![1, 0, 0, 0, 0],
            vec![0, 1, 1, 0, 0],
            vec![0, 0, 0, 1, 0],
            vec![1, 1, 1, 0, 0],
        ],
        'P' => vec![
            vec![1, 1, 1, 0, 0],
            vec![1, 0, 0, 1, 0],
            vec![1, 1, 1, 0, 0],
            vec![1, 0, 0, 0, 0],
            vec![1, 0, 0, 0, 0],
        ],
        _ => return,
    };

    let scale = 4u32;
    let r = (color >> 16) as u8;
    let g = ((color >> 8) & 0xFF) as u8;
    let b = (color & 0xFF) as u8;
    let a = (color >> 24) as u8;

    for py in 0..pattern.len() {
        for px in 0..pattern[py].len() {
            if pattern[py][px] == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let dx = (x as u32 + px as u32 * scale + sx) as u32;
                    let dy = (y as u32 + py as u32 * scale + sy) as u32;
                    let i = ((dy * buf_w + dx) * 4) as usize;
                    if i + 3 < data.len() {
                        data[i] = b;
                        data[i + 1] = g;
                        data[i + 2] = r;
                        data[i + 3] = a;
                    }
                }
            }
        }
    }
}

/// Create a dock icon texture (rounded rect with a letter).
fn create_dock_icon(
    renderer: &mut GlesRenderer,
    size: i32,
    corner_radius: i32,
    color: u32,
    letter: char,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>> {
    let su = size as u32;
    let cru = corner_radius as u32;
    let mut data = vec![0u8; (su * su * 4) as usize];

    for y in 0..su {
        for x in 0..su {
            let dist = if x < cru && y < cru {
                (((cru - x) * (cru - x) + (cru - y) * (cru - y)) as f64).sqrt()
            } else if x >= su - cru && y < cru {
                let dx = if x > su - cru { x - (su - cru) } else { 0 };
                let dy = if y < cru { cru - y } else { 0 };
                ((dx * dx + dy * dy) as f64).sqrt()
            } else if x < cru && y >= su - cru {
                let dx = if x < cru { cru - x } else { 0 };
                let dy = if y > su - cru { y - (su - cru) } else { 0 };
                ((dx * dx + dy * dy) as f64).sqrt()
            } else if x >= su - cru && y >= su - cru {
                let dx = if x > su - cru { x - (su - cru) } else { 0 };
                let dy = if y > su - cru { y - (su - cru) } else { 0 };
                ((dx * dx + dy * dy) as f64).sqrt()
            } else {
                -1.0
            };

            let i = ((y * su + x) * 4) as usize;
            if dist >= 0.0 && dist > cru as f64 {
                data[i + 3] = 0;
            } else {
                data[i] = (color & 0xFF) as u8;
                data[i + 1] = ((color >> 8) & 0xFF) as u8;
                data[i + 2] = ((color >> 16) & 0xFF) as u8;
                data[i + 3] = ((color >> 24) & 0xFF) as u8;
            }
        }
    }

    // Draw letter centered on icon
    draw_letter_bitmap(&mut data, su, ((su as i32 - 20) / 2).max(0), ((su as i32 - 20) / 2).max(0), letter, 0xFFFFFFFF);

    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (size, size), false, tex_scale, Transform::Normal, None).ok()
}

fn create_hover_border(
    renderer: &mut GlesRenderer,
    size: i32,
    corner_radius: i32,
    border_width: i32,
    tex_scale: i32,
) -> Option<TextureBuffer<GlesTexture>> {
    let su = size as u32;
    let cru = corner_radius as u32;
    let bw = border_width as u32;
    let mut data = vec![0u8; (su * su * 4) as usize];

    for y in 0..su { for x in 0..su {
        let dist = if x < cru && y < cru { (((cru - x) * (cru - x) + (cru - y) * (cru - y)) as f64).sqrt() }
            else if x >= su - cru && y < cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y < cru { cru - y } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x < cru && y >= su - cru { let dx = if x < cru { cru - x } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else if x >= su - cru && y >= su - cru { let dx = if x > su - cru { x - (su - cru) } else { 0 }; let dy = if y > su - cru { y - (su - cru) } else { 0 }; ((dx*dx + dy*dy) as f64).sqrt() }
            else { -1.0 };

        let i = ((y * su + x) * 4) as usize;

        if dist >= 0.0 && dist <= bw as f64 {
            let inner_aa = if dist < 1.0 { dist } else { 1.0 };
            let outer_aa = if dist > bw as f64 - 1.0 { bw as f64 - dist } else { 1.0 };
            let alpha = (inner_aa * outer_aa * 160.0).min(255.0) as u8;
            data[i] = 180; data[i+1] = 180; data[i+2] = 180; data[i+3] = alpha;
        } else {
            data[i] = 0; data[i+1] = 0; data[i+2] = 0; data[i+3] = 0;
        }
    }}

    TextureBuffer::from_memory(renderer, &data, Fourcc::Abgr8888, (size, size), false, tex_scale, Transform::Normal, None).ok()
}

fn render_surface(
    surface: &mut SurfaceData,
    renderer: &mut GlesRenderer,
    space: &smithay::desktop::Space<Window>,
    clear_color: [f32; 4],
    wallpaper: Option<&Wallpaper>,
    cursor: Option<&mut crate::cursor::CursorState>,
    pointer_pos: Option<smithay::utils::Point<f64, smithay::utils::Logical>>,
    wallpaper_buffer: &mut Option<TextureBuffer<GlesTexture>>,
    active_app: &Option<String>,
    render_cache: &mut crate::render_cache::RenderCache,
    tontoo_ui: &crate::handlers::tontoo_ui::TontooUiState,
    _focused_surface: Option<&smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    _window_controls: &std::collections::HashMap<String, crate::shell::window_controls::WindowControls>,
) -> Result<(), SwapBuffersError> {
    let output = &surface.output;
    let output_geo = space.output_geometry(output).unwrap_or_default();

    let space_elements = space_render_elements(renderer, std::iter::once(space), output, 1.0)
        .map_err(|_| {
            SwapBuffersError::ContextLost(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get render elements",
            )))
        })?;

    // Get cursor element before building the list, to maintain z-order
    let cursor_element = if let (Some(cs), Some(pos)) = (cursor, pointer_pos) {
        cs.get_cursor_element(renderer, pos)
    } else {
        None
    };

    let (widget_cursor, surface_cursor) = match cursor_element {
        Some(CursorRenderElement::Texture(e)) => (Some(e), None),
        Some(CursorRenderElement::Surface(e)) => (None, Some(e)),
        None => (None, None),
    };

    let mut all_elements: Vec<TontooRenderElements> =
        Vec::with_capacity(space_elements.len() + 10);

    // DRM compositor renders front-to-back (index 0 = topmost).
    // Cursor is inserted at index 0 AFTER all pushes, shifting everything +2.
    // So we push in REVERSE z-order: topmost element first → bottommost last.
    // After cursor insert: [cursor, cursor2, dockbar, space, wallpaper]
    // NOTE: no compositor-side menubar. The top strut is reserved for the
    // external Menubar.app system app (LaunchPad service).
    let output_size = Size::from((output_geo.size.w, output_geo.size.h));

    // 1. Dock (macOS-style glass panel + icons) — rendered at 2x for crisp HiDPI
    {
        const DOCK_H: f32 = 78.0;
        const CORNER_R: f32 = 22.0;
        const BOTTOM_MARGIN: f32 = 15.0;
        const ICON_SIZE: f32 = 48.0;
        const ICON_GAP: f32 = 12.0;
        const ICON_RADIUS: f32 = 12.0;
        const DOCK_SCALE: i32 = 2;

        let sw = output_geo.size.w as f32;
        let sh = output_geo.size.h as f32;
        let icon_count = 5;
        let total_icons_w = ICON_SIZE * icon_count as f32 + ICON_GAP * (icon_count as f32 - 1.0);
        let dw = (total_icons_w + 32.0).min(sw - 40.0);
        let dx = (sw - dw) / 2.0;
        let dy = sh - DOCK_H - BOTTOM_MARGIN;

        let dock_color_scheme = if clear_color[0] < 0.5 { crate::config::ColorScheme::Dark } else { crate::config::ColorScheme::Light };

        // Glass panel (2x resolution, scale=2 for smithay)
        let blur_data = wallpaper.and_then(|wp| {
            let (wp_w, wp_h) = wp.size();
            Some((wp.pixels(), wp_w, wp_h, sw as i32, sh as i32, dx as i32, dy as i32))
        });
        if render_cache.dock_panel.as_ref().map(|(w2, h2, s2, _)| (*w2, *h2, *s2)) != Some((dw as i32 * DOCK_SCALE, (DOCK_H * DOCK_SCALE as f32) as i32, dock_color_scheme)) {
            if let Some(buf) = create_glass_texture(renderer, dw as i32 * DOCK_SCALE, (DOCK_H * DOCK_SCALE as f32) as i32, (CORNER_R * DOCK_SCALE as f32) as i32, blur_data, DOCK_SCALE) {
                render_cache.dock_panel = Some((dw as i32 * DOCK_SCALE, (DOCK_H * DOCK_SCALE as f32) as i32, dock_color_scheme, buf));
            }
        }

        // Icons (2x resolution, scale=2 for smithay)
        let icon_start_x = dx + (dw - total_icons_w) / 2.0;
        let icon_y = dy + (DOCK_H - ICON_SIZE) / 2.0;

        let icon_data = [
            ("Finder", 0xFF2196F3u32, 'F'),
            ("Terminal", 0xFF2979FFu32, 'T'),
            ("Settings", 0xFF9E9E9Eu32, 'S'),
            ("Notes", 0xFF4CAF50u32, 'N'),
            ("Podcasts", 0xFF9C27B0u32, 'P'),
        ];
        for (i, (name, color, letter)) in icon_data.iter().enumerate() {
            let x = icon_start_x + i as f32 * (ICON_SIZE + ICON_GAP);
            let icon_key = (name.to_string(), dock_color_scheme, (ICON_SIZE * DOCK_SCALE as f32) as i32);
            if !render_cache.dock_icons.contains_key(&icon_key) {
                if let Some(buf) = create_dock_icon(renderer, (ICON_SIZE * DOCK_SCALE as f32) as i32, (ICON_RADIUS * DOCK_SCALE as f32) as i32, *color, *letter, DOCK_SCALE) {
                    render_cache.dock_icons.insert(icon_key.clone(), buf);
                }
            }
            if let Some(ref buf) = render_cache.dock_icons.get(&icon_key) {
                let elem = TextureRenderElement::from_texture_buffer(
                    Point::from((x as f64, icon_y as f64)),
                    &buf, None, None,
                    Some(Size::from((ICON_SIZE as i32, ICON_SIZE as i32))),
                    Kind::Unspecified,
                );
                all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
            }

            // Running-app black dot indicator
            let is_active = active_app.as_deref() == Some(*name);
            if is_active {
                let dot_size: f32 = 6.0;
                let dot_x = x + (ICON_SIZE - dot_size) / 2.0;
                let dot_y = icon_y + ICON_SIZE + 3.0;
                let dot_pixel: [u8; 4] = [0x00, 0x00, 0x00, 0xFF]; // black
                if let Ok(dot_buf) = TextureBuffer::from_memory(
                    renderer, &dot_pixel, Fourcc::Abgr8888, (1, 1), false, 1, Transform::Normal, None,
                ) {
                    let elem = TextureRenderElement::from_texture_buffer(
                        Point::from((dot_x as f64, dot_y as f64)),
                        &dot_buf, None, None,
                        Some(Size::from((dot_size as i32, dot_size as i32))),
                        Kind::Unspecified,
                    );
                    all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
                }
            }
        }

        // Glass panel (pushed AFTER icons so it renders below them via .rev() iteration)
        if let Some((_, _, _, ref buf)) = render_cache.dock_panel {
            let elem = TextureRenderElement::from_texture_buffer(
                Point::from((dx as f64, dy as f64)),
                buf, None, None,
                Some(Size::from((dw as i32, DOCK_H as i32))),
                Kind::Unspecified,
            );
            all_elements.push(TontooRenderElements::DockBar(DockBarElement(elem)));
        }
    }

    // Top strut: reserved for the external Menubar.app system app.
    // The compositor renders nothing here; windows are placed below the
    // strut (see handlers/xdg_shell.rs).

    // 2. Client windows + decorations (CSD: shadow + border only, no server titlebar)
    // DRM renders front-to-back: push order = [topmost, ..., bottommost]
    // Cursor is inserted at index 0 later → shifts everything +2
    // So push: borders (topmost) → windows → shadows (bottommost)
    {
        let pad: i32 = 64;
        let _offset_y = WINDOW_SHADOW_OFFSET_Y;

        // Borders disabled - now handled by GTK theme (TontooOS-Dark/Light) for GTK apps only
        // See BaseOS/archiso/airootfs/usr/share/themes/TontooOS-*/gtk-3.0/gtk.css
        // Keeping shadows in compositor for non-GTK windows, but no rounded border mask.
        // for window in space.elements() {
        //     if !window.alive() { continue; }
        //     if let Some(geo) = space.element_geometry(window) {
        //         if geo.size.w <= 0 || geo.size.h <= 0 { continue; }
        //         let border_key = (geo.size.w, geo.size.h, clear_color_to_scheme(clear_color));
        //         if !render_cache.window_borders.contains_key(&border_key) {
        //             if let Some(buf) = create_window_border_mask_texture(renderer, geo.size.w, geo.size.h, clear_color_to_scheme(clear_color)) {
        //                 render_cache.window_borders.insert(border_key, buf);
        //             }
        //         }
        //         if let Some(ref buf) = render_cache.window_borders.get(&border_key) {
        //             let elem = TextureRenderElement::from_texture_buffer(
        //                 Point::from((geo.loc.x as f64, geo.loc.y as f64)),
        //                 buf, None, None,
        //                 Some(Size::from((geo.size.w, geo.size.h))),
        //                 Kind::Unspecified,
        //             );
        //             all_elements.push(TontooRenderElements::WindowBorder(WindowBorderElement(elem)));
        //         }
        //     }
        // }

        // NOTE: Server-side titlebar removed — Client-Side Decorations (CSD) only.
        // Windows (middle layer) — apps include their own header bar
        for elem in space_elements {
            all_elements.push(TontooRenderElements::Space(elem));
        }

        // TontooUI surfaces (declarative widget-tree apps)
        {
            let sw = output_geo.size.w as f32;
            let sh = output_geo.size.h as f32;
            for surf in tontoo_ui.surfaces() {
                if surf.parsed_tree.is_empty() { continue; }
                let surf_w = surf.width as f32;
                let surf_h = surf.height as f32;
                let sx = (sw - surf_w) / 2.0;
                let sy = (sh - surf_h) / 2.0;

                if let Some(glass) = &surf.glass {
                    if let Some(elem) = crate::widget_renderer::WidgetRenderer::render_glass_cmd(
                        renderer, sx, sy, surf_w, surf_h, glass.milkiness, glass.alpha,
                    ) {
                        all_elements.push(TontooRenderElements::TontooUi(
                            crate::cursor::TontooUiTextureElement(elem)));
                    }
                } else {
                    let bg = match surf.color_scheme {
                        crate::handlers::tontoo_ui::TontooColorScheme::Dark =>
                            crate::widget_renderer::Color::new(0.114, 0.114, 0.118, 1.0),
                        crate::handlers::tontoo_ui::TontooColorScheme::Light =>
                            crate::widget_renderer::Color::new(0.925, 0.925, 0.929, 1.0),
                    };
                    if let Some(elem) = crate::widget_renderer::WidgetRenderer::render_rect_cmd(
                        renderer, sx, sy, surf_w, surf_h, bg,
                    ) {
                        all_elements.push(TontooRenderElements::TontooUi(
                            crate::cursor::TontooUiTextureElement(elem)));
                    }
                }

                let cmds = crate::widget_tree::widget_tree_to_draw_commands(
                    &surf.parsed_tree, sx, sy);
                let mut widget_renderer = crate::widget_renderer::WidgetRenderer::new();
                let elems = crate::widget_renderer::render_draw_commands_with(
                    renderer, &cmds, &mut widget_renderer);
                for elem in elems {
                    all_elements.push(TontooRenderElements::TontooUi(
                        crate::cursor::TontooUiTextureElement(elem)));
                }
            }
        }

        // Shadows disabled for now - GTK theme's decoration box-shadow now handles it
        // Compositor shadows were 10px rounded with custom blur, now let GTK's 24px decoration do it
        // for window in space.elements() {
        //     if !window.alive() { continue; }
        //     if let Some(geo) = space.element_geometry(window) {
        //         if geo.size.w <= 0 || geo.size.h <= 0 { continue; }
        //         let shadow_key = (geo.size.w, geo.size.h, clear_color_to_scheme(clear_color));
        //         if !render_cache.window_shadows.contains_key(&shadow_key) {
        //             if let Some(buf) = create_window_shadow_texture(renderer, geo.size.w, geo.size.h, clear_color_to_scheme(clear_color)) {
        //                 render_cache.window_shadows.insert(shadow_key, buf);
        //             }
        //         }
        //         if let Some(ref buf) = render_cache.window_shadows.get(&shadow_key) {
        //             let shadow_pos = Point::from((
        //                 (geo.loc.x - pad) as f64,
        //                 (geo.loc.y as f64) - pad as f64 + offset_y,
        //             ));
        //             let shadow_size = Size::from((geo.size.w + pad * 2, geo.size.h + pad * 2));
        //             let elem = TextureRenderElement::from_texture_buffer(
        //                 shadow_pos, &*buf, None, None,
        //                 Some(shadow_size), Kind::Unspecified,
        //             );
        //             all_elements.push(TontooRenderElements::WindowShadow(WindowShadowElement(elem)));
        //         }
        //     }
        // }
    }

    // 2. Wallpaper (bottommost, pushed last)
    if let Some(wp) = wallpaper {
        if wallpaper_buffer.is_none() {
            *wallpaper_buffer = create_wallpaper_buffer(renderer, wp);
        }
        if let Some(ref buf) = wallpaper_buffer {
            let wp_element = wallpaper_buffer_to_element(buf, wp, output_size);
            all_elements.push(TontooRenderElements::Wallpaper(WallpaperElement(wp_element)));
        }
    }

    // Render elements are composited front-to-back (first = topmost).
    // Insert cursor at index 0 so it is drawn above everything.
    if let Some(e) = widget_cursor {
        all_elements.insert(0, TontooRenderElements::CursorTexture(CursorTextureElement(e)));
    }
    if let Some(e) = surface_cursor {
        all_elements.insert(0, TontooRenderElements::CursorSurface(e));
    }

    let render_frame_result =
        surface
            .compositor
            .render_frame(renderer, &all_elements, clear_color, FrameFlags::empty());

    match render_frame_result {
        Ok(result) => {
            if result.needs_sync() {
                if let PrimaryPlaneElement::Swapchain(element) = &result.primary_element {
                    let _ = element.sync.wait();
                }
            }
            if result.is_empty {
                // Nothing changed since the last presented frame. The static
                // desktop stays on screen; the next timer tick re-checks.
                return Ok(());
            }

            // Send frame callbacks before presenting so clients can start
            // drawing their next frame immediately.
            let time = Clock::<Monotonic>::new().now();
            space.elements().for_each(|window| {
                window.send_frame(output, time, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                });
            });
            let map = layer_map_for_output(output);
            for layer_surface in map.layers() {
                layer_surface.send_frame(output, time, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                });
            }

            // Present the frame synchronously with `commit_frame`.
            //
            // Unlike `queue_frame` + VBlank, this does not depend on page-flip
            // completion events. Virtualized drivers such as vmwgfx (VirtualBox
            // vmsvga) do not deliver reliable VBlank events, which left the
            // swapchain stalled after the very first frame. `commit_frame`
            // performs the commit directly (modeset / atomic commit without a
            // flip event) and does not require `frame_submitted`, so the render
            // loop stays alive purely on the 16ms render timer.
            surface
                .compositor
                .commit_frame()
                .map_err(|e| SwapBuffersError::ContextLost(Box::new(e)))?;
            Ok(())
        }
        Err(e) => {
            Err(SwapBuffersError::ContextLost(Box::new(e)))
        }
    }
}
