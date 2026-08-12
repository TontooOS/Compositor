use std::time::Duration;

/// Tracks the state of a single animation.
#[derive(Debug)]
pub struct Animation {
    /// Total elapsed time for this animation.
    pub elapsed: Duration,
    /// Duration of the animation. `None` means it runs indefinitely.
    pub duration: Option<Duration>,
    /// Whether this animation is still running.
    pub running: bool,
}

impl Animation {
    /// Create a new one-shot animation with a fixed duration.
    pub fn new(duration: Duration) -> Self {
        Self {
            elapsed: Duration::ZERO,
            duration: Some(duration),
            running: true,
        }
    }

    /// Create a new looping animation (no fixed duration).
    pub fn looping() -> Self {
        Self {
            elapsed: Duration::ZERO,
            duration: None,
            running: true,
        }
    }

    /// Current progress as a value in `[0.0, 1.0]`.
    /// Returns `1.0` for one-shot animations that have finished.
    pub fn progress(&self) -> f64 {
        match self.duration {
            Some(dur) if dur.is_zero() => 1.0,
            Some(dur) => (self.elapsed.as_secs_f64() / dur.as_secs_f64()).min(1.0),
            None => 0.0, // looping – caller decides
        }
    }

    /// Returns `true` if the animation is still running.
    pub fn is_active(&self) -> bool {
        self.running
    }
}

/// Manages all active animations in the compositor.
pub struct AnimationManager {
    animations: Vec<Animation>,
}

impl AnimationManager {
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
        }
    }

    /// Register a new animation and return its index.
    pub fn add(&mut self, animation: Animation) -> usize {
        let idx = self.animations.len();
        self.animations.push(animation);
        idx
    }

    /// Stop and remove an animation by index.
    pub fn remove(&mut self, index: usize) {
        if let Some(anim) = self.animations.get_mut(index) {
            anim.running = false;
        }
        self.animations.retain(|a| a.running);
    }

    /// Advance all active animations by `dt` and remove finished ones.
    pub fn tick(&mut self, dt: Duration) {
        for anim in &mut self.animations {
            if !anim.running {
                continue;
            }
            anim.elapsed += dt;
            if let Some(dur) = anim.duration {
                if anim.elapsed >= dur {
                    anim.elapsed = dur;
                    anim.running = false;
                }
            }
        }
        self.animations.retain(|a| a.running);
    }

    /// Returns `true` when at least one animation is still active.
    pub fn has_active(&self) -> bool {
        self.animations.iter().any(|a| a.running)
    }

    /// Number of active animations.
    pub fn active_count(&self) -> usize {
        self.animations.iter().filter(|a| a.running).count()
    }
}

impl Default for AnimationManager {
    fn default() -> Self {
        Self::new()
    }
}
