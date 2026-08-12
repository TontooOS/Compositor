# Launcher

The application launcher overlay is a placeholder for a Spotlight/Launchpad-style
search and launch interface. It is toggled by the shell state.

## Launcher

```rust
pub struct Launcher {
    pub visible: bool,
}
```

### Launcher::new

```rust
pub fn new() -> Self
```

Creates a hidden launcher (`visible = false`).

### Launcher::show

```rust
pub fn show(&mut self)
```

Sets `visible` to `true`.

### Launcher::hide

```rust
pub fn hide(&mut self)
```

Sets `visible` to `false`.

### Launcher::toggle

```rust
pub fn toggle(&mut self)
```

Toggles the `visible` flag.

## Behavior

The launcher currently has no rendering implementation. It is a state holder
that future development will add the search bar, app list, and web search
integration to. The `ShellState::toggle_launcher` method delegates to
`Launcher::toggle`.

## Cross References

- [Shell.md](Shell.md) -- `ShellState::launcher` field
- [State.md](State.md) -- `ShellState::launcher_visible` and
  `ShellState::toggle_launcher`
