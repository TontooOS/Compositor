mod accessibility;
mod animation;
mod config;
mod cursor;
mod display;
mod grabs;
mod handlers;
mod input;
mod protocol;
pub mod render_cache;
mod shaders;
pub mod shell;
mod settings_ipc;
mod state;
mod texture_cache;
mod wallpaper;
pub mod widget_renderer;
pub mod widget_tree;
mod windows_ipc;

#[cfg(feature = "winit")]
mod render;

#[cfg(feature = "udev")]
mod udev;

#[cfg(feature = "udev")]
mod xwayland;

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

pub use state::TontooCompositor;

use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

#[derive(Parser)]
#[command(name = "tontoo-compositor")]
#[command(about = "TontooOS Wayland Compositor")]
struct Args {
    /// Use winit backend (for development inside an existing display server)
    #[arg(long)]
    winit: bool,

    /// Use udev/DRM backend (for running on a TTY)
    #[arg(long)]
    udev: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let args = Args::parse();

    let use_udev = if args.udev {
        true
    } else if args.winit {
        false
    } else {
        #[cfg(feature = "udev")]
        {
            // Default to udev if available and no --winit specified
            true
        }
        #[cfg(not(feature = "udev"))]
        {
            false
        }
    };

    tracing::info!("TontooOS Compositor starting...");

    let mut event_loop: EventLoop<TontooCompositor> = EventLoop::try_new()?;
    let display: Display<TontooCompositor> = Display::new()?;

    let mut state = TontooCompositor::new(&mut event_loop, display);

    // Window listing + actions for CoreWindows (Dock, Mission Control).
    // A bind failure is not fatal: the desktop runs without IPC.
    if let Err(e) = windows_ipc::init(&mut event_loop) {
        tracing::warn!("windows-ipc unavailable: {:?}", e);
    }

    // Desktop settings for the Settings daemon (wallpaper now, display
    // and more later). A bind failure is not fatal either.
    if let Err(e) = settings_ipc::init(&mut event_loop) {
        tracing::warn!("settings-ipc unavailable: {:?}", e);
    }

    if use_udev {
        #[cfg(feature = "udev")]
        {
            tracing::info!("Using udev/DRM backend");
            udev::init_udev(&mut event_loop, &mut state)?;
        }
        #[cfg(not(feature = "udev"))]
        {
            tracing::error!("Udev backend was requested but not compiled into this binary.");
            return Err("udev backend not available".into());
        }
    } else {
        #[cfg(feature = "winit")]
        {
            tracing::info!("Using winit backend");
            render::init_winit(&mut event_loop, &mut state)?;
        }
        #[cfg(not(feature = "winit"))]
        {
            tracing::error!("No backend was compiled (need the winit or udev feature).");
            return Err("no backend compiled".into());
        }
    }

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    tracing::info!(
        "TontooOS Compositor running on {}",
        state.socket_name.to_string_lossy()
    );

    event_loop.run(None, &mut state, move |_| {})?;

    Ok(())
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter.clone());

    // Write to /dev/console so logs appear on QEMU serial console
    if let Ok(console) = std::fs::OpenOptions::new().write(true).open("/dev/console") {
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(console)
            .with_filter(filter);
        tracing_subscriber::Registry::default()
            .with(stderr_layer)
            .with(console_layer)
            .init();
    } else {
        tracing_subscriber::Registry::default()
            .with(stderr_layer)
            .init();
    }
}
