/-!
# Project model

The project is a value, and `step` is the only way to change it (ADR 2). This
file is the specification of the Rust core in `crates/pathos-core` (ADR 3).
-/

namespace Pathos

/-- Musical time in ticks. -/
abbrev Tick := Nat

/-- Ticks per quarter note. -/
def ppq : Nat := 960

/-- No clip or tempo change may lie beyond this tick. -/
def maxTick : Tick := 2 ^ 40

/-- Slowest accepted tempo, in microseconds per quarter note (1 BPM). -/
def maxUsPerQuarter : Nat := 60000000

/-- From `tick` onwards a quarter note lasts `usPerQuarter` microseconds. -/
structure TempoChange where
  tick : Tick
  usPerQuarter : Nat
  deriving Repr, DecidableEq

structure Clip where
  id : Nat
  track : Nat
  start : Tick
  length : Nat
  deriving Repr, DecidableEq

/-- The tick just past the end of the clip. -/
def Clip.stop (c : Clip) : Tick := c.start + c.length

/-- Two clips overlap when they share a track and a tick. -/
def Clip.overlaps (a b : Clip) : Bool :=
  a.track == b.track && a.start < b.stop && b.start < a.stop

/-- A project. The id the next created clip receives is not part of it but of
the history, so that undo does not reissue ids (ADR 10). -/
structure Project where
  trackCount : Nat
  /-- Ordered by tick, starting at tick 0. -/
  tempo : List TempoChange
  /-- Ordered by id. -/
  clips : List Clip
  deriving Repr, DecidableEq

/-- An empty project at 120 BPM. -/
def Project.initial : Project :=
  { trackCount := 0, tempo := [⟨0, 500000⟩], clips := [] }

inductive Command where
  | addTrack
  | addClip (track : Nat) (start : Tick) (length : Nat)
  /-- Move a clip to `track`, starting at `start`. -/
  | moveClip (id : Nat) (track : Nat) (start : Tick)
  /-- Split a clip in two at `tick`, which must lie strictly inside it. -/
  | splitClip (id : Nat) (tick : Tick)
  | deleteClip (id : Nat)
  /-- Set the tempo from `tick` onwards, replacing a change already there. -/
  | setTempo (tick : Tick) (usPerQuarter : Nat)
  | removeTempo (tick : Tick)
  deriving Repr, DecidableEq

inductive Error where
  | emptyClip
  | unknownTrack
  | unknownClip
  | outOfRange
  | overlap
  | splitOutsideClip
  | invalidTempo
  | initialTempo
  | noTempoChange
  | nothingToUndo
  | nothingToRedo
  deriving Repr, DecidableEq

def Project.findClip (p : Project) (id : Nat) : Option Clip :=
  p.clips.find? (·.id == id)

/-- Whether `c` overlaps a clip of the project other than itself. -/
def Project.collides (p : Project) (c : Clip) : Bool :=
  p.clips.any fun other => other.id != c.id && c.overlaps other

def Project.replaceClip (p : Project) (c : Clip) : List Clip :=
  p.clips.map fun other => if other.id == c.id then c else other

/-- Insert a tempo change, keeping the list ordered and one change per tick. -/
def insertTempo (change : TempoChange) : List TempoChange → List TempoChange
  | [] => [change]
  | head :: rest =>
    if change.tick < head.tick then change :: head :: rest
    else if change.tick == head.tick then change :: rest
    else head :: insertTempo change rest

/-- Apply a command to `p`, where `nextClipId` is the id the next created clip
receives. Returns the project and the id after the command. A rejected command
leaves no trace: the caller keeps the project and the id it had. Checks run in
the order written, which fixes the error reported when several apply. -/
def step (p : Project) (nextClipId : Nat) : Command → Except Error (Project × Nat)
  | .addTrack => pure ({ p with trackCount := p.trackCount + 1 }, nextClipId)
  | .addClip track start length => do
    if length == 0 then throw .emptyClip
    if track ≥ p.trackCount then throw .unknownTrack
    if start + length > maxTick then throw .outOfRange
    let clip : Clip := { id := nextClipId, track, start, length }
    if p.collides clip then throw .overlap
    pure ({ p with clips := p.clips ++ [clip] }, nextClipId + 1)
  | .moveClip id track start => do
    let some clip := p.findClip id | throw .unknownClip
    if track ≥ p.trackCount then throw .unknownTrack
    if start + clip.length > maxTick then throw .outOfRange
    let moved := { clip with track, start }
    if p.collides moved then throw .overlap
    pure ({ p with clips := p.replaceClip moved }, nextClipId)
  | .splitClip id tick => do
    let some clip := p.findClip id | throw .unknownClip
    if tick ≤ clip.start || tick ≥ clip.stop then throw .splitOutsideClip
    let left := { clip with length := tick - clip.start }
    let right : Clip :=
      { id := nextClipId, track := clip.track, start := tick, length := clip.stop - tick }
    pure ({ p with clips := p.replaceClip left ++ [right] }, nextClipId + 1)
  | .deleteClip id => do
    let some _ := p.findClip id | throw .unknownClip
    pure ({ p with clips := p.clips.filter (·.id != id) }, nextClipId)
  | .setTempo tick usPerQuarter => do
    if usPerQuarter == 0 || usPerQuarter > maxUsPerQuarter then throw .invalidTempo
    if tick > maxTick then throw .outOfRange
    pure ({ p with tempo := insertTempo ⟨tick, usPerQuarter⟩ p.tempo }, nextClipId)
  | .removeTempo tick => do
    if tick == 0 then throw .initialTempo
    if !p.tempo.any (·.tick == tick) then throw .noTempoChange
    pure ({ p with tempo := p.tempo.filter (·.tick != tick) }, nextClipId)

/-- `ppq` times the microseconds from the first tempo change to `tick`. -/
def microsTimesPpq : List TempoChange → Tick → Nat
  | [], _ => 0
  | [last], tick => (tick - last.tick) * last.usPerQuarter
  | current :: next :: rest, tick =>
    if tick ≤ next.tick then (tick - current.tick) * current.usPerQuarter
    else
      (next.tick - current.tick) * current.usPerQuarter + microsTimesPpq (next :: rest) tick

/-- The time of `tick` in microseconds from the start, rounded down. -/
def Project.tickToMicros (p : Project) (tick : Tick) : Nat :=
  microsTimesPpq p.tempo tick / ppq

/-! ## Invariant -/

/-- Tempo changes start at tick 0, are strictly ordered and are in range. -/
def tempoWellFormed : List TempoChange → Bool
  | [] => false
  | first :: rest =>
    first.tick == 0 && ordered (first :: rest)
where
  ordered : List TempoChange → Bool
    | [] => true
    | [last] => valid last
    | current :: next :: rest =>
      valid current && current.tick < next.tick && ordered (next :: rest)
  valid (change : TempoChange) : Bool :=
    change.usPerQuarter > 0 && change.usPerQuarter ≤ maxUsPerQuarter && change.tick ≤ maxTick

/-- Clips are non-empty, in range, on existing tracks, ordered by id below
`nextClipId`, and no two overlap. -/
def clipsWellFormed (p : Project) (nextClipId : Nat) : Bool :=
  p.clips.all (fun c =>
    c.length > 0 && c.stop ≤ maxTick && c.track < p.trackCount && c.id < nextClipId
      && !p.collides c)
    && ordered p.clips
where
  ordered : List Clip → Bool
    | [] => true
    | [_] => true
    | current :: next :: rest => current.id < next.id && ordered (next :: rest)

/-- `p` is well-formed when the next created clip receives `nextClipId`. -/
def Project.wellFormed (p : Project) (nextClipId : Nat) : Bool :=
  tempoWellFormed p.tempo && clipsWellFormed p nextClipId

/-! ## Properties -/

theorem initial_wellFormed : Project.initial.wellFormed 0 = true := by decide

/-- Every command keeps a well-formed project well-formed. -/
theorem step_preserves_wellFormed (p q : Project) (n n' : Nat) (c : Command)
    (wf : p.wellFormed n = true) (ok : step p n c = .ok (q, n')) :
    q.wellFormed n' = true := by
  sorry

/-- A command never lowers the id the next created clip receives. -/
theorem step_nextClipId_le (p q : Project) (n n' : Nat) (c : Command)
    (ok : step p n c = .ok (q, n')) : n ≤ n' := by
  cases c <;> simp only [step] at ok <;>
    repeat' (first
      | split at ok
      | simp_all [bind, Except.bind, pure, Except.pure, throw, throwThe, MonadExceptOf.throw])
  all_goals omega

/-- Replacing a clip keeps the ids: every clip after it has an id some clip had
before. -/
theorem replaceClip_ids (p : Project) (c x : Clip) (mem : x ∈ p.replaceClip c) :
    x.id ∈ p.clips.map (·.id) := by
  simp only [Project.replaceClip, List.mem_map] at mem ⊢
  obtain ⟨other, mem, eq⟩ := mem
  split at eq
  · rename_i same
    subst eq
    exact ⟨other, mem, by simpa using beq_iff_eq.mp same⟩
  · subst eq
    exact ⟨other, mem, rfl⟩

/-- A clip that a command creates receives an id from `n` up to the returned
one, so no id issued before the command. -/
theorem step_new_clip_ids (p q : Project) (n n' : Nat) (c : Command)
    (ok : step p n c = .ok (q, n')) :
    ∀ clip ∈ q.clips, clip.id ∉ p.clips.map (·.id) → n ≤ clip.id ∧ clip.id < n' := by
  intro clip mem fresh
  -- A clip the command kept or replaced has an id the project had.
  have kept : clip ∉ p.clips := fun mem => fresh (List.mem_map_of_mem mem)
  have replaced : ∀ c, clip ∉ p.replaceClip c :=
    fun c mem => fresh (replaceClip_ids p c clip mem)
  cases c <;> simp only [step] at ok <;>
    repeat' (first
      | split at ok
      | simp_all [bind, Except.bind, pure, Except.pure, throw, throwThe, MonadExceptOf.throw])
  all_goals obtain ⟨rfl, rfl⟩ := ok
  all_goals simp_all

/-- Later ticks are never earlier in time. -/
theorem microsTimesPpq_monotone :
    ∀ (tempo : List TempoChange) (a b : Tick), a ≤ b →
      microsTimesPpq tempo a ≤ microsTimesPpq tempo b
  | [], _, _, _ => Nat.le_refl _
  | [last], a, b, le => by
    simp only [microsTimesPpq]
    exact Nat.mul_le_mul_right _ (Nat.sub_le_sub_right le _)
  | current :: next :: rest, a, b, le => by
    have ih := microsTimesPpq_monotone (next :: rest) a b le
    simp only [microsTimesPpq]
    split <;> split
    · exact Nat.mul_le_mul_right _ (Nat.sub_le_sub_right le _)
    · rename_i before _
      exact Nat.le_trans (Nat.mul_le_mul_right _ (Nat.sub_le_sub_right before _))
        (Nat.le_add_right _ _)
    · rename_i after before
      exact absurd (Nat.le_trans le before) after
    · exact Nat.add_le_add_left ih _

theorem tickToMicros_monotone (p : Project) (a b : Tick) (le : a ≤ b) :
    p.tickToMicros a ≤ p.tickToMicros b :=
  Nat.div_le_div_right (microsTimesPpq_monotone p.tempo a b le)

end Pathos
