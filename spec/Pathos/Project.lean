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

/-- The highest MIDI pitch. -/
def maxPitch : Nat := 127

/-- The highest MIDI velocity. Velocity 0 means note-off in MIDI, so a note's
velocity is at least 1. -/
def maxVelocity : Nat := 127

/-- The loudest track volume, a gain of 2 (ADR 20). -/
def maxVolume : Nat := 2000

/-- The volume of a new track, a gain of 1. -/
def unityVolume : Nat := 1000

/-- The rightmost pan; 0 is leftmost (ADR 21). -/
def maxPan : Nat := 200

/-- The pan of a new track. -/
def centerPan : Nat := 100

/-- A track's volume is its gain in thousandths (ADR 20). A muted track is not
heard, and when any track is soloed, only soloed tracks are (ADR 21). -/
structure Track where
  volume : Nat := unityVolume
  muted : Bool := false
  solo : Bool := false
  pan : Nat := centerPan
  deriving Repr, DecidableEq

/-- From `tick` onwards a quarter note lasts `usPerQuarter` microseconds. -/
structure TempoChange where
  tick : Tick
  usPerQuarter : Nat
  deriving Repr, DecidableEq

/-- A MIDI note, starting `start` ticks after the start of its clip (ADR 14). -/
structure Note where
  start : Tick
  length : Nat
  pitch : Nat
  velocity : Nat
  deriving Repr, DecidableEq

/-- The tick just past the end of the note, relative to its clip. -/
def Note.stop (n : Note) : Tick := n.start + n.length

/-- Two notes overlap when they share a pitch and a tick. -/
def Note.overlaps (a b : Note) : Bool :=
  a.pitch == b.pitch && a.start < b.stop && b.start < a.stop

/-- The order of notes in a clip: by start, then pitch. -/
def Note.before (a b : Note) : Bool :=
  a.start < b.start || (a.start == b.start && a.pitch < b.pitch)

structure Clip where
  id : Nat
  track : Nat
  start : Tick
  length : Nat
  /-- Ordered by start, then pitch. -/
  notes : List Note := []
  deriving Repr, DecidableEq

/-- The tick just past the end of the clip. -/
def Clip.stop (c : Clip) : Tick := c.start + c.length

/-- Two clips overlap when they share a track and a tick. -/
def Clip.overlaps (a b : Clip) : Bool :=
  a.track == b.track && a.start < b.stop && b.start < a.stop

/-- A project. The id the next created clip receives is not part of it but of
the history, so that undo does not reissue ids (ADR 10). -/
structure Project where
  tracks : List Track
  /-- The gain of every note in thousandths, as a track's volume is (ADR 24). -/
  masterVolume : Nat := unityVolume
  /-- Ordered by tick, starting at tick 0. -/
  tempo : List TempoChange
  /-- Ordered by id. -/
  clips : List Clip
  deriving Repr, DecidableEq

def Project.trackCount (p : Project) : Nat := p.tracks.length

/-- An empty project at 120 BPM. -/
def Project.initial : Project :=
  { tracks := [], tempo := [⟨0, 500000⟩], clips := [] }

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
  /-- Add a note to a clip, `start` ticks after the start of the clip. -/
  | addNote (id : Nat) (start : Tick) (length : Nat) (pitch : Nat) (velocity : Nat)
  /-- Remove the note of `pitch` that starts `start` ticks into a clip. -/
  | removeNote (id : Nat) (start : Tick) (pitch : Nat)
  | setTrackVolume (track : Nat) (volume : Nat)
  | setTrackMute (track : Nat) (muted : Bool)
  | setTrackSolo (track : Nat) (solo : Bool)
  | setTrackPan (track : Nat) (pan : Nat)
  | setMasterVolume (volume : Nat)
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
  | emptyNote
  | invalidNote
  | noteOutsideClip
  | noteOverlap
  | unknownNote
  | invalidVolume
  | invalidPan
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

/-- Insert a note, keeping the notes ordered by start, then pitch. -/
def insertNote (note : Note) : List Note → List Note
  | [] => [note]
  | head :: rest =>
    if note.before head then note :: head :: rest
    else head :: insertNote note rest

/-- Apply a command to `p`, where `nextClipId` is the id the next created clip
receives. Returns the project and the id after the command. A rejected command
leaves no trace: the caller keeps the project and the id it had. Checks run in
the order written, which fixes the error reported when several apply. -/
def step (p : Project) (nextClipId : Nat) : Command → Except Error (Project × Nat)
  | .addTrack => pure ({ p with tracks := p.tracks ++ [{}] }, nextClipId)
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
    -- Notes are relative to their clip. A note that starts before the split
    -- stays in the left clip and ends at the split at the latest (ADR 14).
    let offset := tick - clip.start
    let left := { clip with
      length := offset
      notes := clip.notes.filter (·.start < offset) |>.map fun n =>
        { n with length := min n.length (offset - n.start) } }
    let right : Clip :=
      { id := nextClipId, track := clip.track, start := tick, length := clip.stop - tick
        notes := clip.notes.filter (·.start ≥ offset) |>.map fun n =>
          { n with start := n.start - offset } }
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
  | .addNote id start length pitch velocity => do
    let some clip := p.findClip id | throw .unknownClip
    if length == 0 then throw .emptyNote
    if pitch > maxPitch || velocity == 0 || velocity > maxVelocity then throw .invalidNote
    if start + length > clip.length then throw .noteOutsideClip
    let note : Note := { start, length, pitch, velocity }
    if clip.notes.any note.overlaps then throw .noteOverlap
    let clip := { clip with notes := insertNote note clip.notes }
    pure ({ p with clips := p.replaceClip clip }, nextClipId)
  | .removeNote id start pitch => do
    let some clip := p.findClip id | throw .unknownClip
    let named (n : Note) := n.start == start && n.pitch == pitch
    if !clip.notes.any named then throw .unknownNote
    let clip := { clip with notes := clip.notes.filter (!named ·) }
    pure ({ p with clips := p.replaceClip clip }, nextClipId)
  | .setTrackVolume track volume => do
    let some t := p.tracks[track]? | throw .unknownTrack
    if volume > maxVolume then throw .invalidVolume
    pure ({ p with tracks := p.tracks.set track { t with volume } }, nextClipId)
  | .setTrackMute track muted => do
    let some t := p.tracks[track]? | throw .unknownTrack
    pure ({ p with tracks := p.tracks.set track { t with muted } }, nextClipId)
  | .setTrackSolo track solo => do
    let some t := p.tracks[track]? | throw .unknownTrack
    pure ({ p with tracks := p.tracks.set track { t with solo } }, nextClipId)
  | .setTrackPan track pan => do
    let some t := p.tracks[track]? | throw .unknownTrack
    if pan > maxPan then throw .invalidPan
    pure ({ p with tracks := p.tracks.set track { t with pan } }, nextClipId)
  | .setMasterVolume volume => do
    if volume > maxVolume then throw .invalidVolume
    pure ({ p with masterVolume := volume }, nextClipId)

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

/-- Notes are non-empty, inside the clip, valid MIDI, ordered by start then
pitch, and no two of one pitch overlap. -/
def notesWellFormed (c : Clip) : Bool :=
  c.notes.all (fun n =>
    n.length > 0 && n.stop ≤ c.length && n.pitch ≤ maxPitch
      && n.velocity > 0 && n.velocity ≤ maxVelocity)
    && disjoint c.notes && ordered c.notes
where
  disjoint : List Note → Bool
    | [] => true
    | n :: rest => !rest.any n.overlaps && disjoint rest
  ordered : List Note → Bool
    | [] => true
    | [_] => true
    | current :: next :: rest => current.before next && ordered (next :: rest)

/-- Clips are non-empty, in range, on existing tracks, ordered by id below
`nextClipId`, no two overlap, and their notes are well-formed. -/
def clipsWellFormed (p : Project) (nextClipId : Nat) : Bool :=
  p.clips.all (fun c =>
    c.length > 0 && c.stop ≤ maxTick && c.track < p.trackCount && c.id < nextClipId
      && !p.collides c && notesWellFormed c)
    && ordered p.clips
where
  ordered : List Clip → Bool
    | [] => true
    | [_] => true
    | current :: next :: rest => current.id < next.id && ordered (next :: rest)

/-- Every track's volume and pan, and the master volume, are in range. -/
def tracksWellFormed (p : Project) : Bool :=
  p.tracks.all (fun t => t.volume ≤ maxVolume && t.pan ≤ maxPan)
    && p.masterVolume ≤ maxVolume

/-- `p` is well-formed when the next created clip receives `nextClipId`. -/
def Project.wellFormed (p : Project) (nextClipId : Nat) : Bool :=
  tracksWellFormed p && tempoWellFormed p.tempo && clipsWellFormed p nextClipId

/-! ## Properties -/

theorem initial_wellFormed : Project.initial.wellFormed 0 = true := by decide

/-! ### Pairwise forms of the checks

The checks of `wellFormed` walk lists; each is restated as a relation between
every two elements, which `List.Pairwise` lemmas carry through the commands. -/

/-- The disjointness check of notes, as a relation between every two notes. -/
theorem disjoint_iff (l : List Note) :
    notesWellFormed.disjoint l = true ↔ l.Pairwise (fun a b => a.overlaps b = false) := by
  induction l with
  | nil => simp [notesWellFormed.disjoint]
  | cons n rest ih =>
    simp [notesWellFormed.disjoint, ih, List.pairwise_cons]

/-- The order of notes is transitive. -/
theorem Note.before_trans {a b c : Note} (ab : a.before b = true) (bc : b.before c = true) :
    a.before c = true := by
  unfold Note.before at *
  grind

/-- The order check of notes, as a relation between every two notes. -/
theorem notes_ordered_iff (l : List Note) :
    notesWellFormed.ordered l = true ↔ l.Pairwise (fun a b => a.before b = true) := by
  induction l with
  | nil => simp [notesWellFormed.ordered]
  | cons a rest ih =>
    cases rest with
    | nil => simp [notesWellFormed.ordered]
    | cons b rest =>
      simp only [notesWellFormed.ordered, Bool.and_eq_true, ih, List.pairwise_cons]
      constructor
      · rintro ⟨ab, rest_ok⟩
        refine ⟨?_, rest_ok⟩
        intro x mem
        simp only [List.mem_cons] at mem
        rcases mem with rfl | mem
        · exact ab
        · exact Note.before_trans ab (rest_ok.1 x mem)
      · rintro ⟨all, rest_ok⟩
        exact ⟨all b (by simp), rest_ok⟩

/-- The order check of clips, as a relation between every two clips. -/
theorem clips_ordered_iff (l : List Clip) :
    clipsWellFormed.ordered l = true ↔ l.Pairwise (fun a b => a.id < b.id) := by
  induction l with
  | nil => simp [clipsWellFormed.ordered]
  | cons a rest ih =>
    cases rest with
    | nil => simp [clipsWellFormed.ordered]
    | cons b rest =>
      simp only [clipsWellFormed.ordered, Bool.and_eq_true, decide_eq_true_eq, ih,
        List.pairwise_cons]
      constructor
      · rintro ⟨ab, rest_ok⟩
        refine ⟨?_, rest_ok⟩
        intro x mem
        simp only [List.mem_cons] at mem
        rcases mem with rfl | mem
        · exact ab
        · exact Nat.lt_trans ab (rest_ok.1 x mem)
      · rintro ⟨all, rest_ok⟩
        exact ⟨all b (by simp), rest_ok⟩

/-- Overlapping is symmetric. -/
theorem Clip.overlaps_comm (a b : Clip) : a.overlaps b = b.overlaps a := by
  unfold Clip.overlaps
  grind

/-- Two members of a list related pairwise by a symmetric relation are related,
unless they are the same. -/
theorem pairwise_of_mem {α} {R : α → α → Prop} (symm : ∀ a b, R a b → R b a) :
    ∀ {l : List α}, l.Pairwise R → ∀ a ∈ l, ∀ b ∈ l, a ≠ b → R a b
  | [], _, _, mem, _, _, _ => by simp at mem
  | x :: rest, ok, a, ma, b, mb, ne => by
    rw [List.pairwise_cons] at ok
    simp only [List.mem_cons] at ma mb
    rcases ma with rfl | ma <;> rcases mb with rfl | mb
    · exact absurd rfl ne
    · exact ok.1 b mb
    · exact symm _ _ (ok.1 a ma)
    · exact pairwise_of_mem symm ok.2 a ma b mb ne

/-- With distinct ids, no clip colliding with another is the same as no two
clips overlapping. -/
theorem noCollision_iff (l : List Clip) (ids : l.Pairwise (fun a b => a.id < b.id)) :
    (∀ c ∈ l, (l.any fun o => o.id != c.id && c.overlaps o) = false) ↔
      l.Pairwise (fun a b => a.overlaps b = false) := by
  constructor
  · intro none
    refine List.Pairwise.imp_of_mem ?_ ids
    intro a b ma mb lt
    have := none a ma
    simp only [List.any_eq_false, Bool.and_eq_true, bne_iff_ne, ne_eq, not_and,
      Bool.not_eq_true] at this
    exact this b mb (by omega)
  · intro apart c mc
    simp only [List.any_eq_false, Bool.and_eq_true, bne_iff_ne, ne_eq, not_and,
      Bool.not_eq_true]
    intro o mo ne
    exact pairwise_of_mem (fun a b h => by rwa [Clip.overlaps_comm]) apart c mc o mo
      (fun eq => ne (by rw [eq]))

/-- A clip of a well-formed project, on its own. -/
def ClipOk (count n : Nat) (c : Clip) : Prop :=
  c.length > 0 ∧ c.stop ≤ maxTick ∧ c.track < count ∧ c.id < n ∧ notesWellFormed c = true

theorem clipsWellFormed_iff (p : Project) (n : Nat) :
    clipsWellFormed p n = true ↔
      (∀ c ∈ p.clips, ClipOk p.trackCount n c) ∧
        p.clips.Pairwise (fun a b => a.id < b.id) ∧
        p.clips.Pairwise (fun a b => a.overlaps b = false) := by
  unfold clipsWellFormed ClipOk Project.collides
  simp only [Bool.and_eq_true, List.all_eq_true, decide_eq_true_eq, Bool.not_eq_true',
    clips_ordered_iff]
  constructor
  · rintro ⟨each, ids⟩
    refine ⟨fun c mc => ?_, ids, ?_⟩
    · have := each c mc
      exact ⟨this.1.1.1.1.1, this.1.1.1.1.2, this.1.1.1.2, this.1.1.2, this.2⟩
    · exact (noCollision_iff _ ids).mp fun c mc => (each c mc).1.2
  · rintro ⟨each, ids, apart⟩
    refine ⟨fun c mc => ?_, ids⟩
    have := each c mc
    exact ⟨⟨⟨⟨⟨this.1, this.2.1⟩, this.2.2.1⟩, this.2.2.2.1⟩,
      (noCollision_iff _ ids).mpr apart c mc⟩, this.2.2.2.2⟩

/-! ### Tempo -/

/-- The order check of tempo changes, as a relation between every two changes. -/
theorem tempo_ordered_iff (l : List TempoChange) :
    tempoWellFormed.ordered l = true ↔
      (∀ x ∈ l, tempoWellFormed.valid x = true) ∧ l.Pairwise (fun a b => a.tick < b.tick) := by
  induction l with
  | nil => simp [tempoWellFormed.ordered]
  | cons a rest ih =>
    cases rest with
    | nil => simp [tempoWellFormed.ordered]
    | cons b rest =>
      simp only [tempoWellFormed.ordered, Bool.and_eq_true, decide_eq_true_eq, ih,
        List.pairwise_cons, List.mem_cons, forall_eq_or_imp] at ih ⊢
      constructor
      · rintro ⟨⟨va, ab⟩, ⟨vb, vrest⟩, bs, rest_ok⟩
        refine ⟨⟨va, vb, vrest⟩, ⟨ab, fun x mem => Nat.lt_trans ab (bs x mem)⟩, bs, rest_ok⟩
      · rintro ⟨⟨va, vb, vrest⟩, ⟨ab, _⟩, bs, rest_ok⟩
        exact ⟨⟨va, ab⟩, ⟨vb, vrest⟩, bs, rest_ok⟩

/-- Well-formed tempo changes, as a first change at tick 0 and relations. -/
theorem tempoWellFormed_iff (l : List TempoChange) :
    tempoWellFormed l = true ↔
      (∃ first rest, l = first :: rest ∧ first.tick = 0) ∧
        (∀ x ∈ l, tempoWellFormed.valid x = true) ∧ l.Pairwise (fun a b => a.tick < b.tick) := by
  cases l with
  | nil => simp [tempoWellFormed]
  | cons first rest =>
    simp only [tempoWellFormed, Bool.and_eq_true, beq_iff_eq, tempo_ordered_iff]
    constructor
    · rintro ⟨zero, ok⟩
      exact ⟨⟨first, rest, rfl, zero⟩, ok⟩
    · rintro ⟨⟨f, r, eq, zero⟩, ok⟩
      cases eq
      exact ⟨zero, ok⟩

/-- Inserting a tempo change adds only that change. -/
theorem mem_insertTempo {change x : TempoChange} :
    ∀ {l : List TempoChange}, x ∈ insertTempo change l → x = change ∨ x ∈ l
  | [], mem => by simpa [insertTempo] using mem
  | head :: rest, mem => by
    simp only [insertTempo] at mem
    split at mem
    · simpa using mem
    · split at mem
      · simp only [List.mem_cons] at mem ⊢
        rcases mem with h | h
        · exact Or.inl h
        · exact Or.inr (Or.inr h)
      · simp only [List.mem_cons] at mem ⊢
        rcases mem with h | h
        · exact Or.inr (Or.inl h)
        · rcases mem_insertTempo h with h | h
          · exact Or.inl h
          · exact Or.inr (Or.inr h)

/-- Inserting a tempo change keeps the changes strictly ordered by tick. -/
theorem insertTempo_pairwise (change : TempoChange) :
    ∀ {l : List TempoChange}, l.Pairwise (fun a b => a.tick < b.tick) →
      (insertTempo change l).Pairwise (fun a b => a.tick < b.tick)
  | [], _ => by simp [insertTempo]
  | head :: rest, ok => by
    rw [List.pairwise_cons] at ok
    simp only [insertTempo]
    split
    · rename_i lt
      refine List.pairwise_cons.mpr ⟨?_, List.pairwise_cons.mpr ok⟩
      intro x mem
      simp only [List.mem_cons] at mem
      rcases mem with rfl | mem
      · exact lt
      · exact Nat.lt_trans lt (ok.1 x mem)
    · split
      · rename_i _ eq
        refine List.pairwise_cons.mpr ⟨?_, ok.2⟩
        intro x mem
        have := ok.1 x mem
        simp only [beq_iff_eq] at eq
        grind
      · rename_i notLt notEq
        refine List.pairwise_cons.mpr ⟨?_, insertTempo_pairwise change ok.2⟩
        intro x mem
        rcases mem_insertTempo mem with rfl | mem
        · simp only [beq_iff_eq] at notEq
          grind
        · exact ok.1 x mem

/-- Setting a valid tempo keeps the tempo changes well-formed. -/
theorem insertTempo_wellFormed (l : List TempoChange) (change : TempoChange)
    (wf : tempoWellFormed l = true) (valid : tempoWellFormed.valid change = true) :
    tempoWellFormed (insertTempo change l) = true := by
  rw [tempoWellFormed_iff] at wf ⊢
  obtain ⟨⟨first, rest, rfl, zero⟩, each, ok⟩ := wf
  refine ⟨?_, ?_, insertTempo_pairwise change ok⟩
  · simp only [insertTempo]
    split
    · grind
    · split
      · rename_i _ eq
        simp only [beq_iff_eq] at eq
        exact ⟨change, rest, rfl, by grind⟩
      · exact ⟨first, _, rfl, zero⟩
  · intro x mem
    rcases mem_insertTempo mem with rfl | mem
    · exact valid
    · exact each x mem

/-- Removing a tempo change other than the first keeps them well-formed. -/
theorem removeTempo_wellFormed (l : List TempoChange) (tick : Tick) (nonzero : tick ≠ 0)
    (wf : tempoWellFormed l = true) :
    tempoWellFormed (l.filter (·.tick != tick)) = true := by
  rw [tempoWellFormed_iff] at wf ⊢
  obtain ⟨⟨first, rest, rfl, zero⟩, each, ok⟩ := wf
  refine ⟨?_, ?_, ok.sublist List.filter_sublist⟩
  · refine ⟨first, rest.filter (·.tick != tick), ?_, zero⟩
    rw [List.filter_cons_of_pos]
    simp only [bne_iff_ne, ne_eq]
    grind
  · intro x mem
    exact each x (List.mem_filter.mp mem).1

/-! ### Notes -/

/-- A note of a well-formed clip of `length` ticks, on its own. -/
def NoteOk (length : Nat) (n : Note) : Prop :=
  n.length > 0 ∧ n.stop ≤ length ∧ n.pitch ≤ maxPitch ∧ n.velocity > 0 ∧ n.velocity ≤ maxVelocity

/-- Well-formed notes, as each note on its own and relations between notes. -/
theorem notesWellFormed_iff (c : Clip) :
    notesWellFormed c = true ↔
      (∀ n ∈ c.notes, NoteOk c.length n) ∧
        c.notes.Pairwise (fun a b => a.before b = true) ∧
        c.notes.Pairwise (fun a b => a.overlaps b = false) := by
  unfold notesWellFormed NoteOk
  simp only [Bool.and_eq_true, List.all_eq_true, decide_eq_true_eq, disjoint_iff,
    notes_ordered_iff]
  grind

/-- Overlapping is symmetric. -/
theorem Note.overlaps_comm (a b : Note) : a.overlaps b = b.overlaps a := by
  unfold Note.overlaps
  grind

/-- Two notes are apart unless they share a pitch and a tick. -/
theorem Note.overlaps_eq_false (a b : Note) :
    a.overlaps b = false ↔
      ¬(a.pitch = b.pitch ∧ a.start < b.start + b.length ∧ b.start < a.start + a.length) := by
  unfold Note.overlaps Note.stop
  grind

/-- Inserting a note reorders the notes with it. -/
theorem insertNote_perm (note : Note) :
    ∀ (l : List Note), (insertNote note l).Perm (note :: l)
  | [] => by simp [insertNote]
  | head :: rest => by
    simp only [insertNote]
    split
    · exact List.Perm.refl _
    · exact ((insertNote_perm note rest).cons head).trans (List.Perm.swap note head rest)

/-- Inserting a note whose start and pitch no note has keeps the notes ordered. -/
theorem insertNote_ordered (note : Note) :
    ∀ {l : List Note}, (∀ y ∈ l, ¬(y.start = note.start ∧ y.pitch = note.pitch)) →
      l.Pairwise (fun a b => a.before b = true) →
      (insertNote note l).Pairwise (fun a b => a.before b = true)
  | [], _, _ => by simp [insertNote]
  | head :: rest, distinct, ok => by
    rw [List.pairwise_cons] at ok
    simp only [insertNote]
    split
    · rename_i first
      refine List.pairwise_cons.mpr ⟨?_, List.pairwise_cons.mpr ok⟩
      intro x mem
      simp only [List.mem_cons] at mem
      rcases mem with rfl | mem
      · exact first
      · exact Note.before_trans first (ok.1 x mem)
    · rename_i later
      have headFirst : head.before note = true := by
        have := distinct head (by simp)
        unfold Note.before at later ⊢
        grind
      refine List.pairwise_cons.mpr ⟨?_, insertNote_ordered note
        (fun y mem => distinct y (List.mem_cons_of_mem _ mem)) ok.2⟩
      intro x mem
      rcases List.mem_cons.mp ((List.Perm.mem_iff (insertNote_perm note rest)).mp mem) with
        rfl | mem
      · exact headFirst
      · exact ok.1 x (by simpa using mem)

/-- Adding a note that fits the clip and overlaps no note keeps the notes well-formed. -/
theorem insertNote_wellFormed (c : Clip) (note : Note) (wf : notesWellFormed c = true)
    (ok : NoteOk c.length note) (apart : c.notes.any note.overlaps = false) :
    notesWellFormed { c with notes := insertNote note c.notes } = true := by
  rw [notesWellFormed_iff] at wf ⊢
  obtain ⟨each, ordered, disjoint⟩ := wf
  have perm := insertNote_perm note c.notes
  simp only [List.any_eq_false, Bool.not_eq_true] at apart
  refine ⟨?_, ?_, ?_⟩
  · intro x mem
    rcases List.mem_cons.mp ((List.Perm.mem_iff perm).mp mem) with rfl | mem
    · exact ok
    · exact each x (by simpa using mem)
  · refine insertNote_ordered note ?_ ordered
    intro y mem same
    obtain ⟨sameStart, samePitch⟩ := same
    have overlap := apart y mem
    have := (each y mem).1
    simp only [Note.overlaps, Note.stop, sameStart, samePitch, beq_self_eq_true,
      Bool.true_and, Bool.and_eq_false_iff, decide_eq_false_iff_not, Nat.not_lt] at overlap
    unfold NoteOk at ok
    omega
  · refine (List.Perm.pairwise_iff (fun h => by rwa [Note.overlaps_comm]) perm).mpr ?_
    refine List.pairwise_cons.mpr ⟨fun y mem => apart y mem, disjoint⟩

/-- Removing notes keeps the notes well-formed. -/
theorem filterNotes_wellFormed (c : Clip) (keep : Note → Bool) (wf : notesWellFormed c = true) :
    notesWellFormed { c with notes := c.notes.filter keep } = true := by
  rw [notesWellFormed_iff] at wf ⊢
  obtain ⟨each, ordered, disjoint⟩ := wf
  exact ⟨fun x mem => each x (List.mem_filter.mp mem).1, ordered.sublist List.filter_sublist,
    disjoint.sublist List.filter_sublist⟩

/-- The notes of the left clip of a split are well-formed. -/
theorem splitLeft_wellFormed (c r : Clip) (offset : Nat) (wf : notesWellFormed c = true)
    (length : r.length = offset)
    (notes : r.notes = (c.notes.filter (·.start < offset)).map fun n =>
      { n with length := min n.length (offset - n.start) }) :
    notesWellFormed r = true := by
  rw [notesWellFormed_iff] at wf ⊢
  obtain ⟨each, ordered, disjoint⟩ := wf
  rw [notes, length]
  refine ⟨?_, ?_, ?_⟩
  · intro x mem
    simp only [List.mem_map, List.mem_filter, decide_eq_true_eq] at mem
    obtain ⟨n, ⟨mn, early⟩, rfl⟩ := mem
    have := each n mn
    unfold NoteOk Note.stop at this ⊢
    simp only
    grind
  · rw [List.pairwise_map]
    exact (ordered.sublist List.filter_sublist).imp fun h => by simpa [Note.before] using h
  · rw [List.pairwise_map]
    refine (disjoint.sublist List.filter_sublist).imp fun {a b} h => ?_
    rw [Note.overlaps_eq_false] at h ⊢
    simp only
    grind

/-- The notes of the right clip of a split are well-formed. -/
theorem splitRight_wellFormed (c r : Clip) (offset : Nat) (wf : notesWellFormed c = true)
    (length : r.length = c.length - offset)
    (notes : r.notes = (c.notes.filter (·.start ≥ offset)).map fun n =>
      { n with start := n.start - offset }) :
    notesWellFormed r = true := by
  rw [notesWellFormed_iff] at wf ⊢
  obtain ⟨each, ordered, disjoint⟩ := wf
  rw [notes, length]
  refine ⟨?_, ?_, ?_⟩
  · intro x mem
    simp only [List.mem_map, List.mem_filter, decide_eq_true_eq] at mem
    obtain ⟨n, ⟨mn, late⟩, rfl⟩ := mem
    have := each n mn
    unfold NoteOk Note.stop at this ⊢
    simp only
    grind
  · rw [List.pairwise_map]
    refine (ordered.sublist List.filter_sublist).imp_of_mem fun {a b} ma mb h => ?_
    simp only [List.mem_filter, decide_eq_true_eq] at ma mb
    simp only [Note.before, Bool.or_eq_true, Bool.and_eq_true, decide_eq_true_eq,
      beq_iff_eq] at h ⊢
    grind
  · rw [List.pairwise_map]
    refine (disjoint.sublist List.filter_sublist).imp_of_mem fun {a b} ma mb h => ?_
    simp only [List.mem_filter, decide_eq_true_eq] at ma mb
    rw [Note.overlaps_eq_false] at h ⊢
    simp only
    grind

/-! ### Tracks and clips -/

/-- Well-formed tracks, as a check of each track. -/
theorem tracksWellFormed_iff (p : Project) :
    tracksWellFormed p = true ↔
      (∀ t ∈ p.tracks, t.volume ≤ maxVolume ∧ t.pan ≤ maxPan) ∧ p.masterVolume ≤ maxVolume := by
  unfold tracksWellFormed
  simp

/-- A clip stays well-formed with more tracks and a later next id. -/
theorem ClipOk.mono {count count' n n' : Nat} {c : Clip} (ok : ClipOk count n c)
    (more : count ≤ count') (later : n ≤ n') : ClipOk count' n' c := by
  obtain ⟨a, b, c, d, e⟩ := ok
  exact ⟨a, b, by omega, by omega, e⟩

/-- A clip found by its id is a clip of the project with that id. -/
theorem findClip_spec {p : Project} {id : Nat} {clip : Clip} (found : p.findClip id = some clip) :
    clip ∈ p.clips ∧ clip.id = id :=
  ⟨List.mem_of_find?_eq_some found, by simpa using List.find?_some found⟩

/-- Replacing a clip leaves that clip and clips with other ids. -/
theorem mem_replaceClip {p : Project} {c x : Clip} (mem : x ∈ p.replaceClip c) :
    x = c ∨ (x ∈ p.clips ∧ x.id ≠ c.id) := by
  simp only [Project.replaceClip, List.mem_map] at mem
  obtain ⟨o, mo, eq⟩ := mem
  split at eq
  · exact Or.inl eq.symm
  · rename_i other
    subst eq
    exact Or.inr ⟨mo, by simpa using other⟩

/-- Two clips are apart unless they share a track and a tick. -/
theorem Clip.overlaps_eq_false (a b : Clip) :
    a.overlaps b = false ↔
      ¬(a.track = b.track ∧ a.start < b.start + b.length ∧ b.start < a.start + a.length) := by
  unfold Clip.overlaps Clip.stop
  grind

/-- A clip that lies within `c`, on its track, overlaps no clip that `c` does
not. -/
theorem Clip.overlaps_within {a c o : Clip} (track : a.track = c.track)
    (left : c.start ≤ a.start) (right : a.start + a.length ≤ c.start + c.length)
    (apart : c.overlaps o = false) : a.overlaps o = false := by
  rw [Clip.overlaps_eq_false] at *
  grind

/-- Two clips of a well-formed project with different ids do not overlap. -/
theorem apart_of_mem {l : List Clip} (apart : l.Pairwise (fun a b => a.overlaps b = false))
    {a b : Clip} (ma : a ∈ l) (mb : b ∈ l) (ne : b.id ≠ a.id) : a.overlaps b = false :=
  pairwise_of_mem (fun x y h => by rwa [Clip.overlaps_comm]) apart a ma b mb
    (fun eq => ne (by rw [eq]))

/-- Replacing a clip by one with its id that overlaps no other clip keeps the
clips ordered by id and apart. -/
theorem replaceClip_pairwise (p : Project) (c : Clip)
    (ids : p.clips.Pairwise (fun a b => a.id < b.id))
    (apart : p.clips.Pairwise (fun a b => a.overlaps b = false))
    (fresh : ∀ o ∈ p.clips, o.id ≠ c.id → c.overlaps o = false) :
    (p.replaceClip c).Pairwise (fun a b => a.id < b.id) ∧
      (p.replaceClip c).Pairwise (fun a b => a.overlaps b = false) := by
  unfold Project.replaceClip
  rw [List.pairwise_map, List.pairwise_map]
  constructor
  · refine ids.imp fun {a b} lt => ?_
    split <;> split <;> simp_all
  · refine (ids.and apart).imp_of_mem fun {a b} ma mb ⟨lt, ap⟩ => ?_
    split <;> split
    · simp_all
    · rename_i same other
      simp only [beq_iff_eq] at same other
      exact fresh b mb other
    · rename_i other same
      simp only [beq_iff_eq] at same other
      rw [Clip.overlaps_comm]
      exact fresh a ma other
    · exact ap

/-- Every command keeps a well-formed project well-formed. -/
theorem step_preserves_wellFormed (p q : Project) (n n' : Nat) (c : Command)
    (wf : p.wellFormed n = true) (ok : step p n c = .ok (q, n')) :
    q.wellFormed n' = true := by
  unfold Project.wellFormed at wf ⊢
  simp only [Bool.and_eq_true] at wf ⊢
  obtain ⟨⟨tracksOk, tempoOk⟩, clipsOk⟩ := wf
  rw [clipsWellFormed_iff] at clipsOk ⊢
  rw [tracksWellFormed_iff] at tracksOk ⊢
  obtain ⟨each, ids, apart⟩ := clipsOk
  rcases c with _ | ⟨track, start, length⟩ | ⟨id, track, start⟩ | ⟨id, tick⟩ | ⟨id⟩ |
      ⟨tick, us⟩ | ⟨tick⟩ | ⟨id, start, length, pitch, velocity⟩ | ⟨id, start, pitch⟩ |
      ⟨track, volume⟩ | ⟨track, muted⟩ | ⟨track, solo⟩ | ⟨track, pan⟩ | ⟨volume⟩ <;>
    simp only [step, bind, Except.bind, pure, Except.pure, throw, throwThe,
      MonadExceptOf.throw] at ok <;> repeat' split at ok
  all_goals try (simp at ok; done)
  all_goals simp only [Except.ok.injEq, Prod.mk.injEq] at ok
  all_goals obtain ⟨rfl, rfl⟩ := ok
  -- addTrack
  next =>
    refine ⟨⟨⟨?_, tracksOk.2⟩, tempoOk⟩, ?_, ids, apart⟩
    · intro t mt
      simp only [List.mem_append, List.mem_singleton] at mt
      rcases mt with mt | rfl
      · exact tracksOk.1 t mt
      · decide
    · intro c mc
      exact (each c mc).mono (by simp [Project.trackCount]) (Nat.le_refl _)
  -- addClip
  next =>
    rename_i nonEmpty known inRange free
    simp only [beq_iff_eq, ge_iff_le, Nat.not_le, gt_iff_lt, Nat.not_lt,
      Bool.not_eq_true] at nonEmpty known inRange free
    unfold Project.collides at free
    simp only [List.any_eq_false, Bool.and_eq_true, bne_iff_ne, ne_eq, not_and,
      Bool.not_eq_true] at free
    refine ⟨⟨tracksOk, tempoOk⟩, ?_, ?_, ?_⟩
    · intro c mc
      simp only [List.mem_append, List.mem_singleton] at mc
      rcases mc with mc | rfl
      · exact (each c mc).mono (Nat.le_refl _) (Nat.le_succ _)
      · refine ⟨by dsimp only; grind, by dsimp only [Clip.stop]; grind, known,
          Nat.lt_succ_self _, ?_⟩
        simp [notesWellFormed, notesWellFormed.disjoint, notesWellFormed.ordered]
    · rw [List.pairwise_append]
      refine ⟨ids, by simp, ?_⟩
      intro a ma b mb
      simp only [List.mem_singleton] at mb
      subst mb
      exact (each a ma).2.2.2.1
    · rw [List.pairwise_append]
      refine ⟨apart, by simp, ?_⟩
      intro a ma b mb
      simp only [List.mem_singleton] at mb
      subst mb
      rw [Clip.overlaps_comm]
      exact free a ma (by have := (each a ma).2.2.2.1; grind)
  -- moveClip
  next =>
    rename_i clip found known inRange free
    obtain ⟨mem, sameId⟩ := findClip_spec found
    simp only [ge_iff_le, Nat.not_le, gt_iff_lt, Nat.not_lt, Bool.not_eq_true] at known inRange free
    unfold Project.collides at free
    simp only [List.any_eq_false, Bool.and_eq_true, bne_iff_ne, ne_eq, not_and,
      Bool.not_eq_true] at free
    obtain ⟨ids', apart'⟩ :=
      replaceClip_pairwise p { clip with track := track, start := start } ids apart free
    refine ⟨⟨tracksOk, tempoOk⟩, ?_, ids', apart'⟩
    intro c mc
    rcases mem_replaceClip mc with rfl | ⟨mc, _⟩
    · have := each clip mem
      exact ⟨this.1, by dsimp only [Clip.stop]; grind, known, this.2.2.2.1, this.2.2.2.2⟩
    · exact each c mc
  -- splitClip
  next =>
    rename_i clip found inside
    obtain ⟨mem, sameId⟩ := findClip_spec found
    replace inside : clip.start < tick ∧ tick < clip.start + clip.length := by
      simp only [Clip.stop] at inside
      simp at inside
      grind
    have cOk := each clip mem
    have stop := cOk.2.1
    simp only [Clip.stop] at stop
    have others : ∀ o ∈ p.clips, o.id ≠ clip.id → clip.overlaps o = false :=
      fun o mo ne => apart_of_mem apart mem mo ne
    have fresh := replaceClip_pairwise p
      { clip with
        length := tick - clip.start
        notes := clip.notes.filter (·.start < tick - clip.start) |>.map fun n =>
          { n with length := min n.length (tick - clip.start - n.start) } }
      ids apart fun o mo ne =>
        Clip.overlaps_within (c := clip) rfl (Nat.le_refl _) (by dsimp only; grind) (others o mo ne)
    refine ⟨⟨tracksOk, tempoOk⟩, ?_, ?_, ?_⟩
    · intro c mc
      simp only [List.mem_append, List.mem_singleton] at mc
      rcases mc with mc | rfl
      · rcases mem_replaceClip mc with rfl | ⟨mc, _⟩
        · refine ⟨by dsimp only; grind, by dsimp only [Clip.stop]; grind, cOk.2.2.1,
            Nat.lt_succ_of_lt cOk.2.2.2.1, ?_⟩
          exact splitLeft_wellFormed clip _ (tick - clip.start) cOk.2.2.2.2 rfl rfl
        · exact (each c mc).mono (Nat.le_refl _) (Nat.le_succ _)
      · refine ⟨by dsimp only [Clip.stop]; grind, by dsimp only [Clip.stop]; grind, cOk.2.2.1,
          Nat.lt_succ_self _, ?_⟩
        refine splitRight_wellFormed clip _ (tick - clip.start) cOk.2.2.2.2 ?_ rfl
        dsimp only [Clip.stop]
        grind
    · rw [List.pairwise_append]
      refine ⟨fresh.1, by simp, ?_⟩
      intro a ma b mb
      simp only [List.mem_singleton] at mb
      subst mb
      rcases mem_replaceClip ma with rfl | ⟨ma, _⟩
      · exact cOk.2.2.2.1
      · exact (each a ma).2.2.2.1
    · rw [List.pairwise_append]
      refine ⟨fresh.2, by simp, ?_⟩
      intro a ma b mb
      simp only [List.mem_singleton] at mb
      subst mb
      rcases mem_replaceClip ma with rfl | ⟨ma, other⟩
      · rw [Clip.overlaps_eq_false]
        dsimp only [Clip.stop]
        grind
      · rw [Clip.overlaps_comm]
        exact Clip.overlaps_within (c := clip) rfl (by dsimp only; grind)
          (by dsimp only [Clip.stop]; grind) (others a ma other)
  -- deleteClip
  next =>
    exact ⟨⟨tracksOk, tempoOk⟩, fun c mc => each c (List.mem_filter.mp mc).1,
      ids.sublist List.filter_sublist, apart.sublist List.filter_sublist⟩
  -- setTempo
  next =>
    rename_i valid inRange
    refine ⟨⟨tracksOk, insertTempo_wellFormed _ _ tempoOk ?_⟩, each, ids, apart⟩
    simp only [Bool.or_eq_true, beq_iff_eq, decide_eq_true_eq, not_or, Nat.not_lt,
      gt_iff_lt] at valid inRange
    simp only [tempoWellFormed.valid, Bool.and_eq_true, decide_eq_true_eq]
    grind
  -- removeTempo
  next =>
    rename_i nonzero _
    simp only [beq_iff_eq] at nonzero
    exact ⟨⟨tracksOk, removeTempo_wellFormed _ _ nonzero tempoOk⟩, each, ids, apart⟩
  -- addNote
  next =>
    rename_i clip found nonEmpty valid inside free
    obtain ⟨mem, sameId⟩ := findClip_spec found
    have cOk := each clip mem
    simp only [beq_iff_eq, Bool.or_eq_true, decide_eq_true_eq, not_or, Nat.not_lt,
      gt_iff_lt, Bool.not_eq_true] at nonEmpty valid inside free
    have fresh := replaceClip_pairwise p
      { clip with notes := insertNote { start, length, pitch, velocity } clip.notes }
      ids apart fun o mo ne => by
        have := apart_of_mem apart mem mo ne
        exact this
    refine ⟨⟨tracksOk, tempoOk⟩, ?_, fresh.1, fresh.2⟩
    intro c mc
    rcases mem_replaceClip mc with rfl | ⟨mc, _⟩
    · refine ⟨cOk.1, cOk.2.1, cOk.2.2.1, cOk.2.2.2.1, ?_⟩
      refine insertNote_wellFormed clip _ cOk.2.2.2.2 ?_ free
      unfold NoteOk Note.stop
      dsimp only
      grind
    · exact each c mc
  -- removeNote
  next =>
    rename_i clip found _
    obtain ⟨mem, sameId⟩ := findClip_spec found
    have cOk := each clip mem
    have fresh := replaceClip_pairwise p
      { clip with notes := clip.notes.filter fun n => !(n.start == start && n.pitch == pitch) }
      ids apart fun o mo ne => by
        have := apart_of_mem apart mem mo ne
        exact this
    refine ⟨⟨tracksOk, tempoOk⟩, ?_, fresh.1, fresh.2⟩
    intro c mc
    rcases mem_replaceClip mc with rfl | ⟨mc, _⟩
    · exact ⟨cOk.1, cOk.2.1, cOk.2.2.1, cOk.2.2.2.1, filterNotes_wellFormed clip _ cOk.2.2.2.2⟩
    · exact each c mc
  -- setTrackVolume
  next =>
    rename_i t found inRange
    simp only [gt_iff_lt, Nat.not_lt] at inRange
    have mt := List.mem_of_getElem? found
    refine ⟨⟨⟨?_, tracksOk.2⟩, tempoOk⟩, ?_, ids, apart⟩
    · intro x mx
      rcases List.mem_or_eq_of_mem_set mx with mx | rfl
      · exact tracksOk.1 x mx
      · exact ⟨inRange, (tracksOk.1 t mt).2⟩
    · intro c mc
      exact (each c mc).mono (by simp [Project.trackCount]) (Nat.le_refl _)
  -- setTrackMute
  next =>
    rename_i t found
    have mt := List.mem_of_getElem? found
    refine ⟨⟨⟨?_, tracksOk.2⟩, tempoOk⟩, ?_, ids, apart⟩
    · intro x mx
      rcases List.mem_or_eq_of_mem_set mx with mx | rfl
      · exact tracksOk.1 x mx
      · exact tracksOk.1 t mt
    · intro c mc
      exact (each c mc).mono (by simp [Project.trackCount]) (Nat.le_refl _)
  -- setTrackSolo
  next =>
    rename_i t found
    have mt := List.mem_of_getElem? found
    refine ⟨⟨⟨?_, tracksOk.2⟩, tempoOk⟩, ?_, ids, apart⟩
    · intro x mx
      rcases List.mem_or_eq_of_mem_set mx with mx | rfl
      · exact tracksOk.1 x mx
      · exact tracksOk.1 t mt
    · intro c mc
      exact (each c mc).mono (by simp [Project.trackCount]) (Nat.le_refl _)
  -- setTrackPan
  next =>
    rename_i t found inRange
    simp only [gt_iff_lt, Nat.not_lt] at inRange
    have mt := List.mem_of_getElem? found
    refine ⟨⟨⟨?_, tracksOk.2⟩, tempoOk⟩, ?_, ids, apart⟩
    · intro x mx
      rcases List.mem_or_eq_of_mem_set mx with mx | rfl
      · exact tracksOk.1 x mx
      · exact ⟨(tracksOk.1 t mt).1, inRange⟩
    · intro c mc
      exact (each c mc).mono (by simp [Project.trackCount]) (Nat.le_refl _)
  -- setMasterVolume
  next =>
    rename_i inRange
    simp only [gt_iff_lt, Nat.not_lt] at inRange
    exact ⟨⟨⟨tracksOk.1, inRange⟩, tempoOk⟩, each, ids, apart⟩

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

/-- Splitting a clip keeps the onset of every note: some clip after the split
has a note of the same pitch and velocity starting at the same tick (ADR 14). -/
theorem splitClip_keeps_onsets (p q : Project) (n n' id tick : Nat) (clip : Clip)
    (note : Note) (found : p.findClip id = some clip) (mem : note ∈ clip.notes)
    (ok : step p n (.splitClip id tick) = .ok (q, n')) :
    ∃ c ∈ q.clips, ∃ m ∈ c.notes,
      c.start + m.start = clip.start + note.start ∧ m.pitch = note.pitch
        ∧ m.velocity = note.velocity := by
  have inClips : clip ∈ p.clips := List.mem_of_find?_eq_some found
  have sameId : clip.id = id := by
    simpa using List.find?_some found
  simp only [step, found] at ok
  split at ok
  · simp_all [throw, throwThe, MonadExceptOf.throw, bind, Except.bind]
  · simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at ok
    obtain ⟨rfl, rfl⟩ := ok
    rename_i inside
    simp only [Bool.or_eq_true, decide_eq_true_eq, not_or, Nat.not_le] at inside
    by_cases early : note.start < tick - clip.start
    · refine ⟨{ clip with
          length := tick - clip.start
          notes := clip.notes.filter (·.start < tick - clip.start) |>.map fun n =>
            { n with length := min n.length (tick - clip.start - n.start) } }, ?_, ?_⟩
      · refine List.mem_append_left _ (List.mem_map.mpr ⟨clip, inClips, ?_⟩)
        simp [sameId]
      · exact ⟨{ note with length := min note.length (tick - clip.start - note.start) },
          List.mem_map.mpr ⟨note, List.mem_filter.mpr ⟨mem, by simpa using early⟩, rfl⟩,
          rfl, rfl, rfl⟩
    · refine ⟨_, List.mem_append_right _ (List.mem_singleton_self _), ?_⟩
      refine ⟨{ note with start := note.start - (tick - clip.start) },
        List.mem_map.mpr ⟨note, List.mem_filter.mpr ⟨mem, by simpa using early⟩, rfl⟩,
        ?_, rfl, rfl⟩
      have after : tick - clip.start ≤ note.start := Nat.not_lt.mp early
      have inside : clip.start < tick := inside.1
      show tick + (note.start - (tick - clip.start)) = clip.start + note.start
      omega

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
