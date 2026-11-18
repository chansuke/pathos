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

def Event.track : Event → Nat
  | .off _ track _ => track
  | .on _ track _ _ => track

/-- Whether `track` is muted. A track the project does not have is not. -/
def Project.muted (p : Project) (track : Nat) : Bool :=
  (p.tracks[track]?.map (·.muted)).getD false

/-- Whether `track` is soloed. A track the project does not have is not. -/
def Project.soloed (p : Project) (track : Nat) : Bool :=
  (p.tracks[track]?.map (·.solo)).getD false

/-- Whether `track` is heard: it is not muted, and it is soloed if any track
is (ADR 21). -/
def Project.heard (p : Project) (track : Nat) : Bool :=
  !p.muted track && (p.soloed track || !p.tracks.any (·.solo))

/-- The sample of `tick` at `rate` samples per second, rounded down once. -/
def Project.sampleOf (p : Project) (rate : Nat) (tick : Tick) : Nat :=
  microsTimesPpq p.tempo tick * rate / (ppq * 1000000)

/-- The events of every note at `rate` samples per second, in order. A note
that starts and stops at the same sample is not heard and has no events, since
its note-off would come before its note-on. Nor has a note of a track that is
not heard: a muted one (ADR 20), or one not soloed while another is (ADR 21). -/
def Project.schedule (p : Project) (rate : Nat) : List Event :=
  let events := p.clips.flatMap fun clip => clip.notes.flatMap fun note =>
    let on := p.sampleOf rate (clip.start + note.start)
    let off := p.sampleOf rate (clip.start + note.stop)
    if on == off || !p.heard clip.track then []
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

/-- Every event of the schedule belongs to a track that is heard. -/
theorem schedule_heard (p : Project) (rate : Nat) (e : Event)
    (mem : e ∈ p.schedule rate) : p.heard e.track = true := by
  simp only [Project.schedule, List.mem_mergeSort, List.mem_flatMap] at mem
  obtain ⟨clip, -, note, -, mem⟩ := mem
  split at mem
  · simp at mem
  · rename_i heard
    simp only [Bool.or_eq_true, beq_iff_eq, not_or, Bool.not_eq_true] at heard
    simp only [List.mem_cons, List.not_mem_nil, or_false] at mem
    rcases mem with rfl | rfl <;> simpa [Event.track] using heard.2

/-- A muted track is never heard: no event of the schedule belongs to it. -/
theorem schedule_muted_silent (p : Project) (rate : Nat) (e : Event)
    (mem : e ∈ p.schedule rate) : p.muted e.track = false := by
  have := schedule_heard p rate e mem
  simp only [Project.heard, Bool.and_eq_true, Bool.not_eq_true'] at this
  exact this.1

/-- While a track is soloed, only soloed tracks are heard (ADR 21). -/
theorem schedule_solo (p : Project) (rate : Nat) (e : Event)
    (some_solo : p.tracks.any (·.solo) = true) (mem : e ∈ p.schedule rate) :
    p.soloed e.track = true := by
  have heard := schedule_heard p rate e mem
  unfold Project.heard at heard
  rw [some_solo] at heard
  cases h : p.soloed e.track <;> simp_all

/-- A later tick never lies at an earlier sample. -/
theorem sampleOf_monotone (p : Project) (rate : Nat) (a b : Tick) (le : a ≤ b) :
    p.sampleOf rate a ≤ p.sampleOf rate b :=
  Nat.div_le_div_right (Nat.mul_le_mul_right _ (microsTimesPpq_monotone p.tempo a b le))

end Pathos
