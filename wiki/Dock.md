# Dock

> **Removed.** The compositor no longer renders its own dock. The
> `shell::dock` module (`Dock`, `DockIcon`, `DockAnimation`), the
> `DockBar` render element, the dock glass/icon/hover textures and the
> dock input handling (icon clicks, hover, magnification, bounce) were
> deleted. The bottom dock is now the external `Dock.app` system app.

## External Dock.app

The system dock is a standalone TontooOS app built with TBuild from
`TontooProgramms/Dock` (`bundle_id` `com.tontoo.dock`):

- The ISO build stages it as an extracted bundle at
  `/System/Applications/Dock.app` (see `BaseOS/scripts/stage-dock.sh`).
- At boot the `dock` LaunchPad service
  (`System/services/dock.service`, type `sys`) starts it via
  `/usr/local/bin/start-dock.sh`, which waits for the compositor
  Wayland socket and then execs `/usr/bin/tapp
  /System/Applications/Dock.app`. It depends on `compositor` and
  `live-setup` and restarts on crash.
- On installed systems `install-system.sh` writes a per-user variant of
  the service (`user: <username>`); the live ISO uses `liveuser`.
- Language files are staged at `/usr/share/tontoo/dock/lang/`.

## Minimized Windows

Windows minimized via SSD traffic lights or the `minimize_window` IPC
op are unmapped and tracked in `minimized_windows` /
`minimized_icons` (see [State.md](State.md)). The external `Dock.app`
shows them and restores them via the `restore_window` IPC op
(`shell::ssd::restore_minimized` / `untrack_minimized`, see
[WindowsIpc.md](WindowsIpc.md)). The compositor itself draws no
minimized indicator.

## Cross References

- [Shell.md](Shell.md) -- no `Dock` state left on `ShellState`
- [Rendering.md](Rendering.md) -- no `DockBar` element in the z-order
- [Input.md](Input.md) -- no dock icon click handling; `Super+Enter`
  still launches a terminal
- [Menubar.md](Menubar.md) -- external `Menubar.app` system app (same
  pattern: TBuild bundle + LaunchPad service + starter script)
- [WindowsIpc.md](WindowsIpc.md) -- `minimize_window` / `restore_window`
  ops used by `Dock.app`
