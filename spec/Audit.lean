import Lean

/-!
# Theorem audit

Lists every theorem of the `Pathos` library with its statement, and checks the
axioms each one depends on. Run it with `lake -d spec env lean --run Audit.lean`
after `lake -d spec build`.

The list is committed as `theorems.txt`, so that a pull request shows every
theorem it adds, removes or restates, and every proof it finishes or undoes.
Each line marks the theorem `[proved]` or `[unfinished]`: unfinished when it
depends on `sorryAx`. The audit fails when a theorem depends on an axiom other
than the standard ones, or when the library declares an axiom. `sorryAx` is
allowed, since proofs are not required to merge (ADR 4), but CI rejects a
theorem that was proved and is now unfinished (ADR 9).
-/

open Lean Meta

def standardAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

def isPathosDecl (env : Environment) (name : Name) : Bool :=
  match env.getModuleIdxFor? name with
  | some idx => (`Pathos).isPrefixOf env.header.moduleNames[idx.toNat]!
  | none => false

/-- The statement on one line, so that each theorem is one line of the list. -/
def statement (type : Expr) : MetaM String := do
  let text := toString (← ppExpr type)
  return " ".intercalate (text.splitOn "\n" |>.map String.trimAscii |>.map toString)

def audit : MetaM UInt32 := do
  let env ← getEnv
  -- Declarations written in the source have a range; generated ones do not.
  let decls ← env.constants.toList.filterM fun (name, _) =>
    return isPathosDecl env name && (← findDeclarationRanges? name).isSome
  let mut failed := false
  for (name, info) in decls do
    if info matches .axiomInfo _ then
      IO.eprintln s!"{name} is declared as an axiom"
      failed := true
  let theorems := decls.filterMap fun
    | (name, .thmInfo info) => some (name, info.type)
    | _ => none
  let theorems := theorems.toArray.qsort (fun a b => a.1.toString < b.1.toString)
  let mut unfinished := #[]
  for (name, type) in theorems do
    let axioms ← collectAxioms name
    for ax in axioms do
      if ax == ``sorryAx then
        unfinished := unfinished.push name
      else if !standardAxioms.contains ax then
        IO.eprintln s!"{name} depends on the axiom {ax}"
        failed := true
    let status := if axioms.contains ``sorryAx then "unfinished" else "proved"
    IO.println s!"{name} [{status}] : {← statement type}"
  IO.eprintln s!"{theorems.size} theorems, {unfinished.size} unfinished: {unfinished.toList}"
  return if failed then 1 else 0

unsafe def main : IO UInt32 := do
  initSearchPath (← findSysroot)
  enableInitializersExecution
  let env ← importModules #[{ module := `Pathos }] {} (loadExts := true)
  let ctx := { fileName := "<audit>", fileMap := default }
  let (code, _) ← (audit.run' {} {}).toIO ctx { env }
  return code
