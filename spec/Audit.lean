import Lean

/-!
# Theorem and definition audit

Writes two lists, `theorems.txt` and `definitions.txt`, into the directory
given as the argument, or into the current one. Run it with
`cd spec && lake env lean --run Audit.lean` after `lake -d spec build`.

Both lists are committed, so that a pull request shows every theorem it adds,
removes or restates, every proof it finishes or undoes, and every definition it
changes.

Each line of `theorems.txt` marks the theorem `[proved]` or `[unfinished]`:
unfinished when it depends on `sorryAx`. The audit fails when a theorem depends
on an axiom other than the standard ones, or when the library declares an
axiom. `sorryAx` is allowed, since proofs are not required to merge (ADR 4),
but CI rejects a theorem that was proved and is now unfinished (ADR 9).

Each line of `definitions.txt` gives a definition or a type with a fingerprint
of what it says. A change to its body changes the fingerprint, so CI can ask
for human review of the model's definitions as well as of its theorem
statements (ADR 12).
-/

open Lean Meta

def standardAxioms : List Name := [``propext, ``Classical.choice, ``Quot.sound]

def isPathosDecl (env : Environment) (name : Name) : Bool :=
  match env.getModuleIdxFor? name with
  | some idx => (`Pathos).isPrefixOf env.header.moduleNames[idx.toNat]!
  | none => false

/-- Whether the declaration is written in the source. Generated ones, such as
matchers and the equations of a recursive definition, have no range. -/
def isWritten (name : Name) : MetaM Bool :=
  return (← findDeclarationRanges? name).isSome

/-- The statement on one line, so that each theorem is one line of the list. -/
def statement (type : Expr) : MetaM String := do
  let text := toString (← ppExpr type)
  return " ".intercalate (text.splitOn "\n" |>.map String.trimAscii |>.map toString)

def auditTheorems (decls : List (Name × ConstantInfo)) : MetaM (Bool × Array String) := do
  let theorems := decls.filterMap fun
    | (name, .thmInfo info) => some (name, info.type)
    | _ => none
  let theorems := theorems.toArray.qsort (fun a b => a.1.toString < b.1.toString)
  let mut failed := false
  let mut unfinished := #[]
  let mut lines := #[]
  for (name, type) in theorems do
    let axioms ← collectAxioms name
    for ax in axioms do
      if ax == ``sorryAx then
        unfinished := unfinished.push name
      else if !standardAxioms.contains ax then
        IO.eprintln s!"{name} depends on the axiom {ax}"
        failed := true
    let status := if axioms.contains ``sorryAx then "unfinished" else "proved"
    lines := lines.push s!"{name} [{status}] : {← statement type}"
  IO.eprintln s!"{theorems.size} theorems, {unfinished.size} unfinished: {unfinished.toList}"
  return (failed, lines)

/-- The expressions that say what a declaration is: its type, its body, and
the types of its constructors. -/
def meaning (env : Environment) : ConstantInfo → List Expr
  | .inductInfo info =>
    info.type :: info.ctors.filterMap fun ctor => (env.find? ctor).map (·.type)
  | info => info.type :: info.value?.toList

/-- A fingerprint of a declaration and of the generated declarations it uses,
since the body of a `match` or of a recursive definition lives in those. -/
partial def fingerprint (env : Environment) (name : Name) : StateT NameSet MetaM UInt64 := do
  modify (·.insert name)
  let some info := env.find? name | return 0
  let exprs := meaning env info
  let mut hash := exprs.foldl (fun hash e => mixHash hash e.hash) (hash info.levelParams)
  let used := exprs.toArray.flatMap (·.getUsedConstants) |>.qsort (fun a b => a.toString < b.toString)
  for dep in used do
    if (← get).contains dep || !isPathosDecl env dep || (← isWritten dep) then continue
    hash := mixHash hash (← fingerprint env dep)
  return hash

def kind : ConstantInfo → Option String
  | .defnInfo _ => some "def"
  | .opaqueInfo _ => some "opaque"
  | .inductInfo _ => some "type"
  | _ => none

def within (inner outer : DeclarationRange) : Bool :=
  outer.pos.line ≤ inner.pos.line && inner.endPos.line ≤ outer.endPos.line

def auditDefinitions (env : Environment) (decls : List (Name × ConstantInfo)) :
    MetaM (Array String) := do
  -- Lean generates recursors, projections and derived instances inside the
  -- range of a type. They change only when the type does, which its own line
  -- already shows.
  let types ← decls.filterMapM fun
    | (name, .inductInfo _) => return (← findDeclarationRanges? name).map (name, ·.range)
    | _ => return none
  let defs ← decls.filterMapM fun (name, info) => do
    let some kind := kind info | return none
    let some range ← findDeclarationRanges? name | return none
    if types.any fun (type, outer) => type != name && within range.range outer then
      return none
    return some (name, kind)
  let defs := defs.toArray.qsort (fun a b => a.1.toString < b.1.toString)
  defs.mapM fun (name, kind) => do
    let hash ← (fingerprint env name).run' {}
    let hex := String.ofList (Nat.toDigits 16 hash.toNat)
    return s!"{name} [{kind}] {"".pushn '0' (16 - hex.length)}{hex}"

def audit (dir : System.FilePath) : MetaM UInt32 := do
  let env ← getEnv
  let decls ← env.constants.toList.filterM fun (name, _) =>
    return isPathosDecl env name && (← isWritten name)
  let mut failed := false
  for (name, info) in decls do
    if info matches .axiomInfo _ then
      IO.eprintln s!"{name} is declared as an axiom"
      failed := true
  let (unsound, theorems) ← auditTheorems decls
  let definitions ← auditDefinitions env decls
  IO.FS.writeFile (dir / "theorems.txt") ("\n".intercalate theorems.toList ++ "\n")
  IO.FS.writeFile (dir / "definitions.txt") ("\n".intercalate definitions.toList ++ "\n")
  return if failed || unsound then 1 else 0

unsafe def main (args : List String) : IO UInt32 := do
  let dir := args.headD "."
  initSearchPath (← findSysroot)
  enableInitializersExecution
  let env ← importModules #[{ module := `Pathos }] {} (loadExts := true)
  let ctx := { fileName := "<audit>", fileMap := default }
  let (code, _) ← ((audit dir).run' {} {}).toIO ctx { env }
  return code
