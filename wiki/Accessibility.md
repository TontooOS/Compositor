# Accessibility

Accessibility settings are stored as JSON and can disable transparency and
motion effects across the shell.

- Reduce Transparency: disables glass/blur effects, replaces them with solid
  opaque backgrounds.
- Reduce Motion: disables animations (workspace transitions, window
  open/close, and similar).

## AccessibilitySettings

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub reduce_transparency: bool,
    pub reduce_motion: bool,
}
```

The default is both flags `false`.

`reduce_transparency` is consulted by both glass paths: the render loops
skip the widget-tree glass panel and fall back to the solid scheme color, and
the backdrop stream in [Backdrop.md](Backdrop.md) stops capturing, so a client
never receives a `backdrop` event and its glass falls back to a plain
translucent tint.

### AccessibilitySettings::load

```rust
pub fn load() -> Self
```

Reads `<config_dir>/tontoo/accessibility.json` and deserializes it. Any failure
(missing file, unreadable file, malformed JSON) returns the default settings
silently.

### AccessibilitySettings::save

```rust
pub fn save(&self) -> Result<(), Box<dyn std::error::Error>>
```

Creates the `tontoo` config directory if needed and writes the settings as
pretty-printed JSON to `accessibility.json`. Returns `Err` when the config
directory cannot be resolved or the file cannot be written.

## Config File Format

```json
{
  "reduce_transparency": false,
  "reduce_motion": false
}
```

## Cross References

- [State.md](State.md) -- `TontooCompositor::accessibility` field
- [Backdrop.md](Backdrop.md) -- the backdrop stream is disabled by
  `reduce_transparency`
- [WidgetRenderer.md](WidgetRenderer.md) -- the glass panel that
  `reduce_transparency` replaces with a solid fill
- [Animation.md](Animation.md) -- animations are candidates for the
  `reduce_motion` flag
