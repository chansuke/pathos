import Pathos.Project

/-!
# Undo history

Undo and redo move between whole projects. An implementation may store less, as
long as it behaves the same. The id the next created clip receives belongs to
the history, not to a project, so undo and redo keep it and an id is never
given to two clips (ADR 10).
-/

namespace Pathos

structure History where
  /-- Earlier projects, most recent first. -/
  past : List Project
  present : Project
  /-- Undone projects, next to redo first. -/
  future : List Project
  /-- The id the next created clip receives. Undo and redo keep it. -/
  nextClipId : Nat
  deriving Repr, DecidableEq

def History.initial : History :=
  { past := [], present := Project.initial, future := [], nextClipId := 0 }

inductive Edit where
  | apply (command : Command)
  | undo
  | redo
  deriving Repr, DecidableEq

/-- A rejected command changes nothing. An accepted one clears redo. -/
def History.edit (h : History) : Edit → Except Error History
  | .apply command =>
    match step h.present h.nextClipId command with
    | .ok (next, nextClipId) =>
      pure { past := h.present :: h.past, present := next, future := [], nextClipId }
    | .error e => throw e
  | .undo =>
    match h.past with
    | [] => throw .nothingToUndo
    | previous :: past =>
      pure { h with past, present := previous, future := h.present :: h.future }
  | .redo =>
    match h.future with
    | [] => throw .nothingToRedo
    | next :: future =>
      pure { h with past := h.present :: h.past, present := next, future }

/-- Undo after an accepted command restores the project. -/
theorem undo_apply (h h' : History) (c : Command) (ok : h.edit (.apply c) = .ok h') :
    ∃ h'', h'.edit .undo = .ok h'' ∧ h''.present = h.present ∧ h''.past = h.past := by
  simp only [History.edit] at ok
  split at ok
  · cases ok
    exact ⟨_, rfl, rfl, rfl⟩
  · cases ok

/-- Redo after undo restores the whole history. -/
theorem redo_undo (h h' : History) (ok : h.edit .undo = .ok h') :
    h'.edit .redo = .ok h := by
  simp only [History.edit] at ok
  split at ok
  · cases ok
  · rename_i past_eq
    cases ok
    cases h
    cases past_eq
    rfl

/-- Undo after redo restores the whole history. -/
theorem undo_redo (h h' : History) (ok : h.edit .redo = .ok h') :
    h'.edit .undo = .ok h := by
  simp only [History.edit] at ok
  split at ok
  · cases ok
  · rename_i future_eq
    cases ok
    cases h
    cases future_eq
    rfl

/-- No edit lowers the id the next created clip receives, so an id issued
before an undo is not issued again after it. -/
theorem edit_nextClipId_le (h h' : History) (e : Edit) (ok : h.edit e = .ok h') :
    h.nextClipId ≤ h'.nextClipId := by
  cases e with
  | apply c =>
    simp only [History.edit] at ok
    split at ok
    · rename_i accepted
      cases ok
      exact step_nextClipId_le _ _ _ _ _ accepted
    · cases ok
  | undo | redo =>
    simp only [History.edit] at ok
    split at ok
    · cases ok
    · cases ok
      exact Nat.le_refl _

end Pathos
