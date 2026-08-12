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
- [Rendering.md](Rendering.md) -- glass effects are candidates for the
  `reduce_transparency` flag
- [Animation.md](Animation.md) -- animations are candidates for the
  `reduce_motion` flag
