# Animation

The animation module provides a lightweight tick-based animation system used by
the dock, window transitions, and future compositor effects.

## Animation

```rust
pub struct Animation {
    pub elapsed: Duration,
    pub duration: Option<Duration>,
    pub running: bool,
}
```

### Animation::new

```rust
pub fn new(duration: Duration) -> Self
```

Creates a one-shot animation that runs for the given duration. `elapsed`
starts at `Duration::ZERO` and `running` is `true`.

### Animation::looping

```rust
pub fn looping() -> Self
```

Creates a looping animation with no fixed duration. `running` is `true`.

### Animation::progress

```rust
pub fn progress(&self) -> f64
```

Returns a value in `[0.0, 1.0]`. For one-shot animations this is
`elapsed / duration`, clamped at `1.0`. For looping animations this always
returns `0.0` -- the caller is expected to derive its own progress.

Returns `1.0` immediately when `duration` is zero.

### Animation::is_active

```rust
pub fn is_active(&self) -> bool
```

Returns `true` if `running` is `true`.

## AnimationManager

```rust
pub struct AnimationManager {
    animations: Vec<Animation>,
}
```

### AnimationManager::new

```rust
pub fn new() -> Self
```

Creates an empty manager.

### AnimationManager::add

```rust
pub fn add(&mut self, animation: Animation) -> usize
```

Appends the animation and returns its index.

### AnimationManager::remove

```rust
pub fn remove(&mut self, index: usize)
```

Sets the animation's `running` flag to `false`, then retains only running
animations. The index may become stale after this call.

### AnimationManager::tick

```rust
pub fn tick(&mut self, dt: Duration)
```

Advances all active animations by `dt`. When a one-shot animation reaches its
duration, `elapsed` is clamped to `duration` and `running` is set to `false`.
After ticking, all stopped animations are removed from the vector.

### AnimationManager::has_active

```rust
pub fn has_active(&self) -> bool
```

Returns `true` when at least one animation has `running == true`.

### AnimationManager::active_count

```rust
pub fn active_count(&self) -> usize
```

Returns the number of animations with `running == true`.

## Cross References

- [State.md](State.md) -- `TontooCompositor::animation_manager` field
- [Dock.md](Dock.md) -- dock bounce and magnification use `Animation`
- [Rendering.md](Rendering.md) -- animation manager is ticked during render
