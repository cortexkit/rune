//! The focused field's caret blinks: on for half a second, off for half a second. Typing,
//! moving the caret, changing the selection or focusing another field shows it again and
//! restarts the cycle, so the caret never hides while the user works. Each frame that
//! paints a caret asks the host for one wake at the next on or off, so an idle field draws
//! two frames a second and nothing in between.
//!
//! An inactive window draws no caret. A drawn caret stays on without waking the window
//! under reduced motion, before a window focus report (including headless runtimes), and
//! after [`IDLE`] without a change.
use std::hash::{Hash, Hasher};

use super::{RuntimeCore, rect_inst};

/// How long the caret shows, then hides, in each cycle, in seconds of the host clock.
pub(crate) const HALF: f64 = 0.5;
/// After this long without a change the caret stays on and stops waking the host.
pub(crate) const IDLE: f64 = 60.0;

#[derive(Default)]
pub(crate) struct CaretBlink {
    /// The last blinking field and edit state, hashed, with the clock time it last changed.
    started: Option<(u64, f64)>,
    /// A field caret in the current paint, even if its blink phase was off.
    painting: bool,
}

/// What a painted caret does this frame.
pub(crate) struct Phase {
    pub shown: bool,
    /// Seconds until the next change in visibility, when the caret still blinks.
    pub wake: Option<f64>,
}

impl CaretBlink {
    /// Forget a field caret that was absent for a paint, for example during a range selection.
    pub(super) fn finish_paint(&mut self) {
        if !std::mem::take(&mut self.painting) {
            self.reset();
        }
    }

    /// Start the next visible caret shown, even if the window was inactive between paints.
    pub(in crate::runtime) fn reset(&mut self) {
        self.started = None;
    }

    /// The phase at `now` of the caret painted for `field` with `edit`. `blinks` is false while
    /// the drawn caret should hold still, under reduced motion or before a focus report.
    pub(crate) fn phase(
        &mut self,
        field: &str,
        edit: &crate::TextEdit,
        now: f64,
        blinks: bool,
    ) -> Phase {
        self.painting = true;
        if !blinks {
            self.reset();
            return Phase {
                shown: true,
                wake: None,
            };
        }
        let key = caret_key(field, edit);
        let since = match self.started {
            Some((old, since)) if old == key => since,
            _ => {
                self.started = Some((key, now));
                now
            }
        };
        phase_at(now - since)
    }
}

#[cfg(test)]
#[path = "caret_blink_tests.rs"]
mod paint_tests;

/// The phase `elapsed` seconds after the caret last changed.
pub(crate) fn phase_at(elapsed: f64) -> Phase {
    if elapsed >= IDLE {
        return Phase {
            shown: true,
            wake: None,
        };
    }
    // A wake lands at its deadline give or take float error; count it as having arrived.
    let halves = ((elapsed + 1e-6) / HALF).floor();
    let next = ((halves + 1.0) * HALF).min(IDLE);
    Phase {
        shown: (halves as u64).is_multiple_of(2),
        wake: Some(next - elapsed),
    }
}

fn caret_key(field: &str, edit: &crate::TextEdit) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    field.hash(&mut h);
    (edit.anchor, edit.focus, edit.text.len()).hash(&mut h);
    edit.preedit().hash(&mut h);
    h.finish()
}

impl RuntimeCore {
    pub(super) fn paint_caret(
        &mut self,
        i: usize,
        rect: [f32; 4],
        ink: [f32; 4],
        clip: [f32; 4],
        fade: [f32; 4],
    ) {
        let n = &self.tree.nodes[i];
        if let Some(inp) = &n.input {
            let blinks = self.host.window_focused == Some(true)
                && self.live_visible()
                && !self.reduced_motion;
            let phase = self
                .paint
                .caret_blink
                .phase(&n.path, &inp.edit, self.clock, blinks);
            if let Some(dt) = phase.wake {
                // The loop fast path only updates existing instances. A blink needs a paint to add
                // or remove the caret's instance.
                self.paint.note_cycle_other_deadline();
                self.motion
                    .arm_loop_wake(self.clock + dt, self.t + dt * f64::from(self.time_scale));
            }
            if !phase.shown {
                return;
            }
        }
        self.instances.push(rect_inst(rect, ink, 0.0, clip, fade));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_then_hides_each_half_second() {
        let at = |t: f64| phase_at(t);
        assert!(at(0.0).shown);
        assert!(at(0.49).shown);
        assert!(!at(0.5).shown, "off at the half");
        assert!(!at(0.99).shown);
        assert!(at(1.0).shown);
        assert!(
            (at(0.2).wake.unwrap() - 0.3).abs() < 1e-9,
            "wakes at the next change"
        );
        assert!(
            !at(0.5 - 1e-9).shown,
            "a wake landing a hair early counts as arrived"
        );
    }

    #[test]
    fn stops_on_after_idle() {
        let last = phase_at(IDLE - 0.1);
        assert!(
            (last.wake.unwrap() - 0.1).abs() < 1e-9,
            "the last wake lands at the idle limit"
        );
        let idle = phase_at(IDLE);
        assert!(idle.shown && idle.wake.is_none());
    }

    #[test]
    fn a_change_restarts_the_cycle() {
        let mut b = CaretBlink::default();
        let mut edit = crate::TextEdit::new("ab");
        edit.focused = true;
        assert!(b.phase("f", &edit, 10.0, true).shown);
        assert!(!b.phase("f", &edit, 10.6, true).shown);
        edit.set_caret(1);
        let p = b.phase("f", &edit, 10.7, true);
        assert!(p.shown, "moving the caret shows it");
        assert!((p.wake.unwrap() - HALF).abs() < 1e-9);
        assert!(
            b.phase("g", &edit, 11.3, true).shown,
            "another field starts on"
        );
        assert!(
            b.phase("g", &edit, 20.0, false).shown,
            "held still while it can't blink"
        );
        assert!(
            b.phase("g", &edit, 20.1, true).shown,
            "and starts on when it can again"
        );
    }
}
