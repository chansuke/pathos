import Pathos.Project
import Pathos.WellFormed

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

def Event.sample : Event → Nat
  | .off sample _ _ => sample
  | .on sample _ _ _ => sample

def Event.pitch : Event → Nat
  | .off _ _ pitch => pitch
  | .on _ _ pitch _ => pitch

def Event.isOn (track pitch : Nat) : Event → Bool
  | .on _ t q _ => t == track && q == pitch
  | .off _ _ _ => false

def Event.isOff (track pitch : Nat) : Event → Bool
  | .off _ t q => t == track && q == pitch
  | .on _ _ _ _ => false

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

/-- The sample of `tick` at `rate` samples per second, rounded once to the
nearest sample, a half up (ADR 32). -/
def Project.sampleOf (p : Project) (rate : Nat) (tick : Tick) : Nat :=
  (microsTimesPpq p.tempo tick * rate + ppq * 500000) / (ppq * 1000000)

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
  Nat.div_le_div_right
    (Nat.add_le_add_right (Nat.mul_le_mul_right _ (microsTimesPpq_monotone p.tempo a b le)) _)

/-! ## Note-on and note-off pairing -/

/-- Events ordered by `Event.le` are ordered by sample. -/
theorem Event.sample_le_of_le {a b : Event} (h : a.le b = true) :
    a.sample ≤ b.sample := by
  cases a with
  | off s1 _ _ =>
    cases b with
    | off s2 _ _ =>
      simp only [Event.le, Event.key, Event.sample, compare, compareLex, compareOn,
        compareOfLessAndEq] at h ⊢
      by_cases hlt : s1 < s2
      · exact Nat.le_of_lt hlt
      · by_cases heq : s1 = s2
        · exact Nat.le_of_eq heq
        · simp [hlt, heq] at h
    | on s2 _ _ _ =>
      simp only [Event.le, Event.key, Event.sample, compare, compareLex, compareOn,
        compareOfLessAndEq] at h ⊢
      by_cases hlt : s1 < s2
      · exact Nat.le_of_lt hlt
      · by_cases heq : s1 = s2
        · exact Nat.le_of_eq heq
        · simp [hlt, heq] at h
  | on s1 _ _ _ =>
    cases b with
    | off s2 _ _ =>
      simp only [Event.le, Event.key, Event.sample, compare, compareLex, compareOn,
        compareOfLessAndEq] at h ⊢
      by_cases hlt : s1 < s2
      · exact Nat.le_of_lt hlt
      · by_cases heq : s1 = s2
        · exact Nat.le_of_eq heq
        · simp [hlt, heq] at h
    | on s2 _ _ _ =>
      simp only [Event.le, Event.key, Event.sample, compare, compareLex, compareOn,
        compareOfLessAndEq] at h ⊢
      by_cases hlt : s1 < s2
      · exact Nat.le_of_lt hlt
      · by_cases heq : s1 = s2
        · exact Nat.le_of_eq heq
        · simp [hlt, heq] at h

/-- A note-on comes before or at a note-off in `Event.le` only at a strictly
earlier sample, since note-offs precede note-ons at the same sample. -/
theorem Event.lt_of_on_le_off {s t q v s' t' q' : Nat}
    (h : (Event.on s t q v).le (Event.off s' t' q') = true) : s < s' := by
  simp only [Event.le, Event.key, compare, compareLex, compareOn, compareOfLessAndEq] at h
  by_cases hlt : s < s'
  · exact hlt
  · by_cases heq : s = s' <;> simp [hlt, heq] at h

/-- Every note-off in the schedule is preceded by a note-on of the same track
and pitch at a strictly earlier sample. -/
theorem schedule_off_follows_on (p : Project) (rate j s track pitch : Nat)
    (off : (p.schedule rate)[j]? = some (.off s track pitch)) :
    ∃ i < j, ∃ s' < s, ∃ velocity,
      (p.schedule rate)[i]? = some (.on s' track pitch velocity) := by
  have memOff : Event.off s track pitch ∈ p.schedule rate := List.mem_of_getElem? off
  have hraw : ∃ s' < s, ∃ velocity, Event.on s' track pitch velocity ∈ p.schedule rate := by
    simp only [Project.schedule, List.mem_mergeSort, List.mem_flatMap] at memOff ⊢
    obtain ⟨clip, mc, note, mn, memNote⟩ := memOff
    split at memNote
    · simp at memNote
    · rename_i active
      simp only [Bool.or_eq_true, beq_iff_eq, not_or, Bool.not_eq_true] at active
      simp only [List.mem_cons, List.not_mem_nil, or_false] at memNote
      rcases memNote with h | h
      · cases h
      · cases h
        have hle : p.sampleOf rate (clip.start + note.start) ≤
            p.sampleOf rate (clip.start + note.stop) :=
          sampleOf_monotone p rate (clip.start + note.start) (clip.start + note.stop)
            (Nat.add_le_add_left (Nat.le_add_right note.start note.length) clip.start)
        refine ⟨p.sampleOf rate (clip.start + note.start), Nat.lt_of_le_of_ne hle active.1,
          note.velocity, clip, mc, note, mn, ?_⟩
        split
        · rename_i h'
          simp only [Bool.or_eq_true, beq_iff_eq, Bool.not_eq_true'] at h'
          rcases h' with h' | h'
          · exact absurd h' active.1
          · simp [h'] at active
        · simp
  obtain ⟨s', hlt, velocity, memOn⟩ := hraw
  obtain ⟨i, hi, eqOn⟩ := List.getElem_of_mem memOn
  have getOn : (p.schedule rate)[i]? = some (.on s' track pitch velocity) := by
    rw [List.getElem?_eq_getElem hi, eqOn]
  have hij : i < j := by
    rcases Nat.lt_or_ge i j with hij | hjle
    · exact hij
    · rcases Nat.eq_or_lt_of_le hjle with rfl | hjlt
      · rw [getOn] at off
        cases off
      · have pw := (List.pairwise_iff_getElem.mp (schedule_ordered p rate)) j i
          (Nat.lt_trans hjlt hi) hi hjlt
        have eqOff : (p.schedule rate)[j]'(Nat.lt_trans hjlt hi) = Event.off s track pitch := by
          have := List.getElem?_eq_getElem (Nat.lt_trans hjlt hi)
          rw [this] at off
          exact Option.some.inj off
        rw [eqOff, eqOn] at pw
        have hle := Event.sample_le_of_le pw
        simp only [Event.sample] at hle
        exact absurd hle (Nat.not_le_of_gt hlt)
  exact ⟨i, hij, s', hlt, velocity, getOn⟩

/-- Every track and pitch has as many note-ons as note-offs in the schedule. -/
theorem schedule_on_off_count (p : Project) (rate track pitch : Nat) :
    (p.schedule rate).countP (Event.isOn track pitch) =
      (p.schedule rate).countP (Event.isOff track pitch) := by
  have flatMap_eq : ∀ {α β : Type _} (f : α → List β) (P Q : β → Bool),
      (∀ a, (f a).countP P = (f a).countP Q) →
      ∀ (l : List α), (l.flatMap f).countP P = (l.flatMap f).countP Q := by
    intro α β f P Q h l
    induction l with
    | nil => simp
    | cons a rest ih =>
      simp only [List.flatMap_cons, List.countP_append]
      rw [h a, ih]
  unfold Project.schedule
  rw [(List.mergeSort_perm _ _).countP_eq, (List.mergeSort_perm _ _).countP_eq]
  apply flatMap_eq
  intro clip
  apply flatMap_eq
  intro note
  dsimp only
  split
  · simp
  · rw [List.countP_cons, List.countP_cons, List.countP_nil,
      List.countP_cons, List.countP_cons, List.countP_nil]
    simp [Event.isOn, Event.isOff]

/-- In a list ordered by `Event.le`, any non-empty subset picked out by `P` has a
greatest element under `Event.le`. -/
theorem max_of_pairwise_le {P : Event → Bool} :
    ∀ {l : List Event}, l.Pairwise (fun a b => a.le b = true) → 0 < l.countP P →
      ∃ e ∈ l, P e = true ∧ ∀ x ∈ l, P x = true → x.le e = true
  | [], _, pos => by simp at pos
  | a :: rest, pw, pos => by
    rw [List.pairwise_cons] at pw
    by_cases hrest : 0 < rest.countP P
    · obtain ⟨e, me, pe, hmax⟩ := max_of_pairwise_le pw.2 hrest
      refine ⟨e, List.mem_cons_of_mem _ me, pe, ?_⟩
      intro x mx px
      rcases List.mem_cons.mp mx with rfl | mx
      · exact pw.1 e me
      · exact hmax x mx px
    · have hzero : rest.countP P = 0 := Nat.eq_zero_of_not_pos hrest
      rw [List.countP_cons, hzero] at pos
      have pa : P a = true := by
        cases h : P a
        · simp [h] at pos
        · rfl
      refine ⟨a, List.mem_cons_self .., pa, ?_⟩
      intro x mx px
      rcases List.mem_cons.mp mx with rfl | mx
      · simp [Event.le, Std.ReflOrd.compare_self]
      · have := List.countP_eq_zero.mp hzero x mx
        simp [px] at this

private theorem countP_pair_off_le_on (t track q pitch sOn sOff s v : Nat) (hlt : sOn < sOff) :
    ([Event.on sOn t q v, Event.off sOff t q]).countP
      (fun x => Event.isOff track pitch x && decide (x.sample ≤ s)) ≤
    ([Event.on sOn t q v, Event.off sOff t q]).countP
      (fun x => Event.isOn track pitch x && decide (x.sample < s)) := by
  simp only [List.countP_cons, List.countP_nil, Event.isOff, Event.isOn,
    Bool.false_and, Bool.false_eq_true, ↓reduceIte, Nat.add_zero, Nat.zero_add]
  change (if (t == track && q == pitch && decide (sOff ≤ s)) = true then 1 else 0) ≤
    (if (t == track && q == pitch && decide (sOn < s)) = true then 1 else 0)
  by_cases h1 : (t == track && q == pitch && decide (sOff ≤ s)) = true
  · have h1' := h1
    simp only [Bool.and_eq_true, decide_eq_true_eq] at h1'
    have h2 : (t == track && q == pitch && decide (sOn < s)) = true := by
      simp only [Bool.and_eq_true, decide_eq_true_eq]
      exact ⟨h1'.1, Nat.lt_of_lt_of_le hlt h1'.2⟩
    simp only [h1, h2, ↓reduceIte]
    exact Nat.le_refl 1
  · simp only [h1]
    exact Nat.zero_le _

private theorem countP_pair_on_le_off_add (t track q pitch sOn sOff s v : Nat) :
    ([Event.on sOn t q v, Event.off sOff t q]).countP
      (fun x => Event.isOn track pitch x && decide (x.sample ≤ s)) ≤
    ([Event.on sOn t q v, Event.off sOff t q]).countP
      (fun x => Event.isOff track pitch x && decide (x.sample ≤ s)) +
    (if t == track && (q == pitch && decide (sOn ≤ s) && decide (s < sOff)) then 1 else 0) := by
  simp only [List.countP_cons, List.countP_nil, Event.isOn, Event.isOff,
    Bool.false_and, Bool.false_eq_true, ↓reduceIte, Nat.add_zero, Nat.zero_add]
  change (if (t == track && q == pitch && decide (sOn ≤ s)) = true then 1 else 0) ≤
    (if (t == track && q == pitch && decide (sOff ≤ s)) = true then 1 else 0) +
    (if (t == track && (q == pitch && decide (sOn ≤ s) && decide (s < sOff))) = true then 1 else 0)
  by_cases hon : (t == track && q == pitch && decide (sOn ≤ s)) = true
  · by_cases hoff : (t == track && q == pitch && decide (sOff ≤ s)) = true
    · simp only [hon, hoff, ↓reduceIte]
      exact Nat.le_add_right 1 _
    · have hon' := hon
      simp only [Bool.and_eq_true, decide_eq_true_eq] at hon'
      have hoff' := hoff
      simp only [Bool.and_eq_true, decide_eq_true_eq, not_and] at hoff'
      have hgt : s < sOff := Nat.lt_of_not_ge (hoff' hon'.1)
      have hact : (t == track && (q == pitch && decide (sOn ≤ s) && decide (s < sOff))) = true := by
        simp only [Bool.and_eq_true, decide_eq_true_eq]
        exact ⟨hon'.1.1, ⟨⟨hon'.1.2, hon'.2⟩, hgt⟩⟩
      simp only [hon, hoff, hact, ↓reduceIte]
      exact Nat.le_refl 1
  · simp only [hon]
    exact Nat.zero_le _

/-- Every note-off of `(track, pitch)` at or before sample `s` is matched by a
note-on strictly before sample `s`. -/
theorem schedule_countP_off_le_on_lt (p : Project) (rate track pitch s : Nat) :
    (p.schedule rate).countP (fun x => Event.isOff track pitch x && decide (x.sample ≤ s)) ≤
      (p.schedule rate).countP (fun x => Event.isOn track pitch x && decide (x.sample < s)) := by
  have flatMap_le : ∀ {α β : Type _} (f : α → List β) (P Q : β → Bool),
      (∀ a, (f a).countP P ≤ (f a).countP Q) →
      ∀ (l : List α), (l.flatMap f).countP P ≤ (l.flatMap f).countP Q := by
    intro α β f P Q h l
    induction l with
    | nil => simp
    | cons a rest ih =>
      simp only [List.flatMap_cons, List.countP_append]
      exact Nat.add_le_add (h a) ih
  unfold Project.schedule
  rw [(List.mergeSort_perm _ _).countP_eq, (List.mergeSort_perm _ _).countP_eq]
  apply flatMap_le
  intro clip
  apply flatMap_le
  intro note
  dsimp only
  split
  · simp
  · rename_i active
    simp only [Bool.or_eq_true, beq_iff_eq, not_or, Bool.not_eq_true] at active
    have hle : p.sampleOf rate (clip.start + note.start) ≤
        p.sampleOf rate (clip.start + note.stop) :=
      sampleOf_monotone p rate (clip.start + note.start) (clip.start + note.stop)
        (Nat.add_le_add_left (Nat.le_add_right note.start note.length) clip.start)
    have hlt : p.sampleOf rate (clip.start + note.start) <
        p.sampleOf rate (clip.start + note.stop) :=
      Nat.lt_of_le_of_ne hle active.1
    exact countP_pair_off_le_on clip.track track note.pitch pitch
      (p.sampleOf rate (clip.start + note.start))
      (p.sampleOf rate (clip.start + note.stop)) s note.velocity hlt

/-- In every prefix of the schedule, no track and pitch has more note-offs than
note-ons. -/
theorem schedule_prefix_off_le_on (p : Project) (rate track pitch k : Nat) :
    ((p.schedule rate).take k).countP (Event.isOff track pitch) ≤
      ((p.schedule rate).take k).countP (Event.isOn track pitch) := by
  by_cases hpos : 0 < ((p.schedule rate).take k).countP (Event.isOff track pitch)
  · have hsplit : (p.schedule rate).take k ++ (p.schedule rate).drop k = p.schedule rate :=
      List.take_append_drop k (p.schedule rate)
    have hpw : ((p.schedule rate).take k ++ (p.schedule rate).drop k).Pairwise
        (fun a b => a.le b = true) := by
      rw [hsplit]
      exact schedule_ordered p rate
    rw [List.pairwise_append] at hpw
    obtain ⟨e, me, _, hmax⟩ := max_of_pairwise_le hpw.1 hpos
    let s := e.sample
    let isOffLe : Event → Bool := fun x => Event.isOff track pitch x && decide (x.sample ≤ s)
    let isOnLt : Event → Bool := fun x => Event.isOn track pitch x && decide (x.sample < s)
    have hpreOff : ((p.schedule rate).take k).countP (Event.isOff track pitch) =
        ((p.schedule rate).take k).countP isOffLe := by
      apply List.countP_congr
      intro x mx
      simp only [isOffLe]
      cases hx : Event.isOff track pitch x
      · simp
      · simpa using Event.sample_le_of_le (hmax x mx hx)
    have hsufOn : ((p.schedule rate).drop k).countP isOnLt = 0 := by
      rw [List.countP_eq_zero]
      intro y my
      simp only [isOnLt, Bool.and_eq_true, decide_eq_true_eq, not_and, Nat.not_lt]
      intro _
      exact Event.sample_le_of_le (hpw.2.2 e me y my)
    have h1 : ((p.schedule rate).take k).countP (Event.isOff track pitch) ≤
        (p.schedule rate).countP isOffLe := by
      have hcount := congrArg (List.countP isOffLe) hsplit.symm
      rw [List.countP_append] at hcount
      rw [hpreOff, hcount]
      exact Nat.le_add_right _ _
    have h2 : (p.schedule rate).countP isOffLe ≤ (p.schedule rate).countP isOnLt :=
      schedule_countP_off_le_on_lt p rate track pitch s
    have h3 : (p.schedule rate).countP isOnLt ≤
        ((p.schedule rate).take k).countP (Event.isOn track pitch) := by
      have hcount := congrArg (List.countP isOnLt) hsplit.symm
      rw [List.countP_append, hsufOn, Nat.add_zero] at hcount
      rw [hcount]
      exact List.countP_mono_left (fun x _ hx => (Bool.and_eq_true .. |>.mp hx).1)
    exact Nat.le_trans h1 (Nat.le_trans h2 h3)
  · exact Nat.le_trans (Nat.le_of_eq (Nat.eq_zero_of_not_pos hpos)) (Nat.zero_le _)

/-- In a well-formed project, at any sample `s`, note-ons of `(track, pitch)` at
or before `s` exceed note-offs at or before `s` by at most one. -/
theorem schedule_countP_on_le_off_succ (p : Project) (n rate track pitch s : Nat)
    (wf : p.wellFormed n = true) :
    (p.schedule rate).countP (fun x => Event.isOn track pitch x && decide (x.sample ≤ s)) ≤
      (p.schedule rate).countP (fun x => Event.isOff track pitch x && decide (x.sample ≤ s)) + 1 := by
  have countP_le_one : ∀ {α : Type _} {P : α → Bool} {l : List α},
      l.Pairwise (fun a b => !(P a && P b)) → l.countP P ≤ 1 := by
    intro α P l pw
    induction l with
    | nil => simp
    | cons a rest ih =>
      rw [List.pairwise_cons] at pw
      rw [List.countP_cons]
      cases ha : P a
      · simpa using ih pw.2
      · have hzero : rest.countP P = 0 := by
          rw [List.countP_eq_zero]
          intro b mb
          have := pw.1 b mb
          simp [ha] at this
          simp [this]
        simp [hzero]
  have flatMap_le : ∀ {α β : Type _} (f : α → List β) (P Q : β → Bool) (R : α → Nat),
      (∀ a, (f a).countP P ≤ (f a).countP Q + R a) →
      ∀ (l : List α), (l.flatMap f).countP P ≤ (l.flatMap f).countP Q + (l.map R).sum := by
    intro α β f P Q R h l
    induction l with
    | nil => simp
    | cons a rest ih =>
      simp only [List.flatMap_cons, List.countP_append, List.map_cons, List.sum_cons]
      have ha := h a
      omega
  have hwfClips : clipsWellFormed p n = true := by
    unfold Project.wellFormed at wf
    simp only [Bool.and_eq_true] at wf
    exact wf.2
  obtain ⟨each, _, apart⟩ := (clipsWellFormed_iff p n).mp hwfClips
  let isOnLe : Event → Bool := fun x => Event.isOn track pitch x && decide (x.sample ≤ s)
  let isOffLe : Event → Bool := fun x => Event.isOff track pitch x && decide (x.sample ≤ s)
  let noteActive (c : Clip) (note : Note) : Bool :=
    note.pitch == pitch &&
      decide (p.sampleOf rate (c.start + note.start) ≤ s) &&
      decide (s < p.sampleOf rate (c.start + note.stop))
  let clipActive (c : Clip) : Bool :=
    c.track == track && c.notes.any (noteActive c)
  have hclipCount : p.clips.countP clipActive ≤ 1 := by
    apply countP_le_one
    refine apart.imp_of_mem fun {a b} ma mb hab => ?_
    cases ha : clipActive a
    · simp
    · cases hb : clipActive b
      · simp
      · exfalso
        simp only [clipActive, Bool.and_eq_true, beq_iff_eq, List.any_eq_true] at ha hb
        obtain ⟨hta, na, mna, hna⟩ := ha
        obtain ⟨htb, nb, mnb, hnb⟩ := hb
        have hnaOk := ((notesWellFormed_iff a).mp (each a ma).2.2.2.2).1 na mna
        have hnbOk := ((notesWellFormed_iff b).mp (each b mb).2.2.2.2).1 nb mnb
        simp only [noteActive, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at hna hnb
        rw [Clip.overlaps_eq_false] at hab
        have hnoOverlap : ¬(a.start < b.start + b.length ∧ b.start < a.start + a.length) :=
          fun h => hab ⟨hta.trans htb.symm, h⟩
        have hcases : a.start + a.length ≤ b.start ∨ b.start + b.length ≤ a.start := by
          rcases Nat.lt_or_ge b.start (a.start + a.length) with h1 | h1
          · rcases Nat.lt_or_ge a.start (b.start + b.length) with h2 | h2
            · exact absurd ⟨h2, h1⟩ hnoOverlap
            · exact Or.inr h2
          · exact Or.inl h1
        rcases hcases with hle | hle
        · have hnaStop : na.stop ≤ a.length := hnaOk.2.1
          have htick : a.start + na.stop ≤ b.start + nb.start :=
            Nat.le_trans (Nat.add_le_add_left hnaStop a.start)
              (Nat.le_trans hle (Nat.le_add_right b.start nb.start))
          have hmon := sampleOf_monotone p rate (a.start + na.stop) (b.start + nb.start) htick
          exact absurd (Nat.le_trans hmon hnb.1.2) (Nat.not_le_of_gt hna.2)
        · have hnbStop : nb.stop ≤ b.length := hnbOk.2.1
          have htick : b.start + nb.stop ≤ a.start + na.start :=
            Nat.le_trans (Nat.add_le_add_left hnbStop b.start)
              (Nat.le_trans hle (Nat.le_add_right a.start na.start))
          have hmon := sampleOf_monotone p rate (b.start + nb.stop) (a.start + na.start) htick
          exact absurd (Nat.le_trans hmon hna.1.2) (Nat.not_le_of_gt hnb.2)
  let noteEvents (clip : Clip) (note : Note) : List Event :=
    if (p.sampleOf rate (clip.start + note.start) ==
          p.sampleOf rate (clip.start + note.stop) || !p.heard clip.track) then []
    else [.on (p.sampleOf rate (clip.start + note.start)) clip.track note.pitch note.velocity,
      .off (p.sampleOf rate (clip.start + note.stop)) clip.track note.pitch]
  have hsum : ∀ (cs : List Clip),
      (∀ c ∈ cs, ClipOk p.trackCount n c) →
      (cs.flatMap fun clip => clip.notes.flatMap (noteEvents clip)).countP isOnLe ≤
      (cs.flatMap fun clip => clip.notes.flatMap (noteEvents clip)).countP isOffLe +
        cs.countP clipActive := by
    intro cs hcs
    induction cs with
    | nil => simp
    | cons clip rest ih =>
      simp only [List.flatMap_cons, List.countP_append, List.countP_cons]
      have hclipOk := hcs clip (List.mem_cons_self ..)
      have hdisj := ((notesWellFormed_iff clip).mp hclipOk.2.2.2.2).2.2
      have hnoteCount : clip.notes.countP (noteActive clip) ≤ 1 := by
        apply countP_le_one
        refine hdisj.imp fun {a b} hab => ?_
        cases ha : noteActive clip a
        · simp
        · cases hb : noteActive clip b
          · simp
          · exfalso
            simp only [noteActive, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at ha hb
            rw [Note.overlaps_eq_false] at hab
            have hnoOverlap :
                ¬(a.start < b.start + b.length ∧ b.start < a.start + a.length) :=
              fun h => hab ⟨ha.1.1.trans hb.1.1.symm, h⟩
            have hcases : a.stop ≤ b.start ∨ b.stop ≤ a.start := by
              unfold Note.stop
              rcases Nat.lt_or_ge b.start (a.start + a.length) with h1 | h1
              · rcases Nat.lt_or_ge a.start (b.start + b.length) with h2 | h2
                · exact absurd ⟨h2, h1⟩ hnoOverlap
                · exact Or.inr h2
              · exact Or.inl h1
            rcases hcases with hle | hle
            · have hmon := sampleOf_monotone p rate (clip.start + a.stop) (clip.start + b.start)
                (Nat.add_le_add_left hle clip.start)
              exact absurd (Nat.le_trans hmon hb.1.2) (Nat.not_le_of_gt ha.2)
            · have hmon := sampleOf_monotone p rate (clip.start + b.stop) (clip.start + a.start)
                (Nat.add_le_add_left hle clip.start)
              exact absurd (Nat.le_trans hmon ha.1.2) (Nat.not_le_of_gt hb.2)
      have hnotes := flatMap_le
        (noteEvents clip)
        isOnLe isOffLe
        (fun note => if clip.track == track && noteActive clip note then 1 else 0)
        (fun note => by
          simp only [noteEvents]
          split
          · simp
          · exact countP_pair_on_le_off_add clip.track track note.pitch pitch
              (p.sampleOf rate (clip.start + note.start))
              (p.sampleOf rate (clip.start + note.stop)) s note.velocity)
        clip.notes
      have hmapLe : (clip.notes.map fun note =>
          if clip.track == track && noteActive clip note then 1 else 0).sum ≤
          if clipActive clip then 1 else 0 := by
        by_cases ht : clip.track = track
        · have hsumEq : ∀ (ns : List Note),
              (ns.map fun note => if clip.track == track && noteActive clip note then 1 else 0).sum =
              ns.countP (noteActive clip) := by
            intro ns
            induction ns with
            | nil => simp
            | cons x xs ihx =>
              rw [List.map_cons, List.sum_cons, List.countP_cons, ihx]
              simp only [ht, beq_self_eq_true, Bool.true_and]
              cases noteActive clip x <;> simp [Nat.add_comm]
          rw [hsumEq]
          simp only [clipActive, ht, beq_self_eq_true, Bool.true_and]
          cases hany : clip.notes.any (noteActive clip)
          · simp only [Bool.false_eq_true, ↓reduceIte]
            have hzero : clip.notes.countP (noteActive clip) = 0 := by
              rw [List.countP_eq_zero]
              intro x mx
              have := List.any_eq_false.mp hany x mx
              simp [this]
            exact Nat.le_of_eq hzero
          · simpa using hnoteCount
        · have hzero : (clip.notes.map fun note =>
              if clip.track == track && noteActive clip note then 1 else 0).sum = 0 := by
            induction clip.notes with
            | nil => simp
            | cons x xs ihx =>
              rw [List.map_cons, List.sum_cons, ihx]
              simp [ht]
          rw [hzero]
          exact Nat.zero_le _
      have hclipBound := Nat.le_trans hnotes (Nat.add_le_add_left hmapLe _)
      have hrest := ih (fun c mc => hcs c (List.mem_cons_of_mem _ mc))
      have hcomb := Nat.add_le_add hclipBound hrest
      have hreassoc :
          ((clip.notes.flatMap (noteEvents clip)).countP isOffLe +
              if clipActive clip then 1 else 0) +
            ((rest.flatMap fun clip => clip.notes.flatMap (noteEvents clip)).countP isOffLe +
              rest.countP clipActive) =
          ((clip.notes.flatMap (noteEvents clip)).countP isOffLe +
              (rest.flatMap fun clip => clip.notes.flatMap (noteEvents clip)).countP isOffLe) +
            (rest.countP clipActive + if clipActive clip then 1 else 0) := by
        ac_rfl
      rw [hreassoc] at hcomb
      exact hcomb
  unfold Project.schedule
  rw [(List.mergeSort_perm _ _).countP_eq, (List.mergeSort_perm _ _).countP_eq]
  exact Nat.le_trans (hsum p.clips each) (Nat.add_le_add_left hclipCount _)

/-- In every prefix of the schedule of a well-formed project, no track and pitch
has more than one sounding note: note-ons exceed note-offs by at most one. -/
theorem schedule_prefix_on_le_off_succ (p : Project) (n rate track pitch k : Nat)
    (wf : p.wellFormed n = true) :
    ((p.schedule rate).take k).countP (Event.isOn track pitch) ≤
      ((p.schedule rate).take k).countP (Event.isOff track pitch) + 1 := by
  by_cases hpos : 0 < ((p.schedule rate).take k).countP (Event.isOn track pitch)
  · have hsplit : (p.schedule rate).take k ++ (p.schedule rate).drop k = p.schedule rate :=
      List.take_append_drop k (p.schedule rate)
    have hpw : ((p.schedule rate).take k ++ (p.schedule rate).drop k).Pairwise
        (fun a b => a.le b = true) := by
      rw [hsplit]
      exact schedule_ordered p rate
    rw [List.pairwise_append] at hpw
    obtain ⟨e, me, pe, hmax⟩ := max_of_pairwise_le hpw.1 hpos
    cases e with
    | off _ _ _ => simp [Event.isOn] at pe
    | on s t q v =>
      let isOnLe : Event → Bool := fun x => Event.isOn track pitch x && decide (x.sample ≤ s)
      let isOffLe : Event → Bool := fun x => Event.isOff track pitch x && decide (x.sample ≤ s)
      have hpreOn : ((p.schedule rate).take k).countP (Event.isOn track pitch) =
          ((p.schedule rate).take k).countP isOnLe := by
        apply List.countP_congr
        intro x mx
        simp only [isOnLe]
        cases hx : Event.isOn track pitch x
        · simp
        · have hle : x.sample ≤ s := Event.sample_le_of_le (hmax x mx hx)
          simpa [Bool.true_and, decide_eq_true_eq] using hle
      have hsufOff : ((p.schedule rate).drop k).countP isOffLe = 0 := by
        rw [List.countP_eq_zero]
        intro y my
        cases y with
        | on _ _ _ _ => simp [isOffLe, Event.isOff]
        | off s' t' q' =>
          change ¬(t' == track && q' == pitch && decide (s' ≤ s)) = true
          simp only [Bool.and_eq_true, decide_eq_true_eq, not_and]
          intro _
          exact Nat.not_le_of_gt (Event.lt_of_on_le_off (hpw.2.2 _ me _ my))
      have h1 : ((p.schedule rate).take k).countP (Event.isOn track pitch) ≤
          (p.schedule rate).countP isOnLe := by
        have hcount := congrArg (List.countP isOnLe) hsplit.symm
        rw [List.countP_append] at hcount
        rw [hpreOn, hcount]
        exact Nat.le_add_right _ _
      have h2 : (p.schedule rate).countP isOnLe ≤ (p.schedule rate).countP isOffLe + 1 :=
        schedule_countP_on_le_off_succ p n rate track pitch s wf
      have h3 : (p.schedule rate).countP isOffLe ≤
          ((p.schedule rate).take k).countP (Event.isOff track pitch) := by
        have hcount := congrArg (List.countP isOffLe) hsplit.symm
        rw [List.countP_append, hsufOff, Nat.add_zero] at hcount
        rw [hcount]
        exact List.countP_mono_left (fun x _ hx => (Bool.and_eq_true .. |>.mp hx).1)
      exact Nat.le_trans h1 (Nat.le_trans h2 (Nat.add_le_add_right h3 1))
  · exact Nat.le_trans (Nat.le_of_eq (Nat.eq_zero_of_not_pos hpos)) (Nat.zero_le _)

/-- In a well-formed project, just before any note-on of `(track, pitch)`, no
note of that track and pitch is sounding. -/
theorem schedule_before_on_silent (p : Project) (n rate j s track pitch velocity : Nat)
    (wf : p.wellFormed n = true)
    (on : (p.schedule rate)[j]? = some (.on s track pitch velocity)) :
    ((p.schedule rate).take j).countP (Event.isOn track pitch) =
      ((p.schedule rate).take j).countP (Event.isOff track pitch) := by
  have ⟨hj, _⟩ := List.getElem?_eq_some_iff.mp on
  have htake : (p.schedule rate).take (j + 1) =
      (p.schedule rate).take j ++ [.on s track pitch velocity] := by
    rw [List.take_succ_eq_append_getElem hj]
    have eq := List.getElem?_eq_getElem hj
    rw [eq] at on
    rw [Option.some.inj on]
  have hle := schedule_prefix_off_le_on p rate track pitch j
  have hge := schedule_prefix_on_le_off_succ p n rate track pitch (j + 1) wf
  rw [htake, List.countP_append, List.countP_append] at hge
  simp [Event.isOn, Event.isOff] at hge
  omega

/-- In a well-formed project, just before any note-off of `(track, pitch)`,
exactly one note of that track and pitch is sounding. -/
theorem schedule_before_off_sounding (p : Project) (n rate j s track pitch : Nat)
    (wf : p.wellFormed n = true)
    (off : (p.schedule rate)[j]? = some (.off s track pitch)) :
    ((p.schedule rate).take j).countP (Event.isOn track pitch) =
      ((p.schedule rate).take j).countP (Event.isOff track pitch) + 1 := by
  have ⟨hj, _⟩ := List.getElem?_eq_some_iff.mp off
  have htake : (p.schedule rate).take (j + 1) =
      (p.schedule rate).take j ++ [.off s track pitch] := by
    rw [List.take_succ_eq_append_getElem hj]
    have eq := List.getElem?_eq_getElem hj
    rw [eq] at off
    rw [Option.some.inj off]
  have hle := schedule_prefix_on_le_off_succ p n rate track pitch j wf
  have hge := schedule_prefix_off_le_on p rate track pitch (j + 1)
  rw [htake, List.countP_append, List.countP_append] at hge
  simp [Event.isOn, Event.isOff] at hge
  omega

end Pathos
