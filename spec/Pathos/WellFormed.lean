import Pathos.Project

/-!
# Well-formedness

Every command keeps a well-formed project well-formed. The checks of
`Project.wellFormed` walk lists; each is restated as a relation between every
two elements, which `List.Pairwise` lemmas carry through the commands.
-/

namespace Pathos

/-! ## Pairwise forms of the checks -/

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

/-! ## Tempo -/

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

/-! ## Notes -/

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

/-! ## Tracks and clips -/

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

/-! ## Commands -/

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

end Pathos
