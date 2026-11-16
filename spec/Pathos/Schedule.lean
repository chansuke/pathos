import Pathos.Project

/-!
# Note schedule

The samples at which notes start and stop in a render (ADR 15). The Rust render
turns this schedule into sound; the model specifies only where each note lies.
-/

namespace Pathos

attribute [local instance] lexOrd

/-- A note starting or stopping at a sample of the render. -/
inductive Event where
  | off (sample track pitch : Nat)
  | on (sample track pitch velocity : Nat)
  deriving Repr, DecidableEq

/-- The order of events: by sample, then a note-off before a note-on, so that a
note may start where another of the same pitch stops, then by track, pitch and
velocity. Two events with the same key are equal. -/
def Event.key : Event → Nat × Nat × Nat × Nat × Nat
  | .off sample track pitch => (sample, 0, track, pitch, 0)
  | .on sample track pitch velocity => (sample, 1, track, pitch, velocity)

def Event.le (a b : Event) : Bool := (compare a.key b.key).isLE

/-- The sample of `tick` at `rate` samples per second, rounded down once. -/
def Project.sampleOf (p : Project) (rate : Nat) (tick : Tick) : Nat :=
  microsTimesPpq p.tempo tick * rate / (ppq * 1000000)

/-- The events of every note at `rate` samples per second, in order. A note
that starts and stops at the same sample is not heard and has no events, since
its note-off would come before its note-on. -/
def Project.schedule (p : Project) (rate : Nat) : List Event :=
  let events := p.clips.flatMap fun clip => clip.notes.flatMap fun note =>
    let on := p.sampleOf rate (clip.start + note.start)
    let off := p.sampleOf rate (clip.start + note.stop)
    if on == off then []
    else [.on on clip.track note.pitch note.velocity, .off off clip.track note.pitch]
  events.mergeSort Event.le

/-- The schedule is in the order of `Event.key`. -/
theorem schedule_ordered (p : Project) (rate : Nat) :
    (p.schedule rate).Pairwise fun a b => a.le b = true := by
  apply List.pairwise_mergeSort
  · intro a b c ab bc
    simp only [Event.le] at *
    exact Std.TransOrd.isLE_trans ab bc
  · intro a b
    simp only [Event.le, Bool.or_eq_true]
    cases h : compare a.key b.key
    · simp
    · simp
    · right
      rw [Std.OrientedOrd.eq_swap (a := b.key) (b := a.key), h]
      rfl

/-- A later tick never lies at an earlier sample. -/
theorem sampleOf_monotone (p : Project) (rate : Nat) (a b : Tick) (le : a ≤ b) :
    p.sampleOf rate a ≤ p.sampleOf rate b :=
  Nat.div_le_div_right (Nat.mul_le_mul_right _ (microsTimesPpq_monotone p.tempo a b le))

end Pathos
