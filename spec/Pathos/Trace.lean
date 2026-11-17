import Pathos.History
import Pathos.Schedule

/-!
# Trace format

A line-based text format for driving the model from outside. The differential
tests of the Rust core send one request per line and compare each reply with
their own (ADR 4).
-/

namespace Pathos

inductive Request where
  | edit (edit : Edit)
  /-- Ask for the time of a tick in microseconds. -/
  | micros (tick : Tick)
  /-- Ask for the note schedule at a sample rate (ADR 15). -/
  | schedule (rate : Nat)

def Request.parse (line : String) : Option Request := do
  let words := line.splitOn " " |>.filter (· ≠ "")
  let name :: arguments := words | none
  let numbers ← arguments.mapM String.toNat?
  match name, numbers with
  | "add-track", [] => some (.edit (.apply .addTrack))
  | "add-clip", [track, start, length] => some (.edit (.apply (.addClip track start length)))
  | "move-clip", [id, track, start] => some (.edit (.apply (.moveClip id track start)))
  | "split-clip", [id, tick] => some (.edit (.apply (.splitClip id tick)))
  | "delete-clip", [id] => some (.edit (.apply (.deleteClip id)))
  | "set-tempo", [tick, usPerQuarter] => some (.edit (.apply (.setTempo tick usPerQuarter)))
  | "remove-tempo", [tick] => some (.edit (.apply (.removeTempo tick)))
  | "add-note", [id, start, length, pitch, velocity] =>
    some (.edit (.apply (.addNote id start length pitch velocity)))
  | "remove-note", [id, start, pitch] => some (.edit (.apply (.removeNote id start pitch)))
  | "set-track-volume", [track, volume] =>
    some (.edit (.apply (.setTrackVolume track volume)))
  | "mute-track", [track] => some (.edit (.apply (.setTrackMute track true)))
  | "unmute-track", [track] => some (.edit (.apply (.setTrackMute track false)))
  | "solo-track", [track] => some (.edit (.apply (.setTrackSolo track true)))
  | "unsolo-track", [track] => some (.edit (.apply (.setTrackSolo track false)))
  | "set-track-pan", [track, pan] => some (.edit (.apply (.setTrackPan track pan)))
  | "undo", [] => some (.edit .undo)
  | "redo", [] => some (.edit .redo)
  | "micros", [tick] => some (.micros tick)
  | "schedule", [rate] => some (.schedule rate)
  | _, _ => none

def Error.name : Error → String
  | .emptyClip => "empty-clip"
  | .unknownTrack => "unknown-track"
  | .unknownClip => "unknown-clip"
  | .outOfRange => "out-of-range"
  | .overlap => "overlap"
  | .splitOutsideClip => "split-outside-clip"
  | .invalidTempo => "invalid-tempo"
  | .initialTempo => "initial-tempo"
  | .noTempoChange => "no-tempo-change"
  | .emptyNote => "empty-note"
  | .invalidNote => "invalid-note"
  | .noteOutsideClip => "note-outside-clip"
  | .noteOverlap => "note-overlap"
  | .unknownNote => "unknown-note"
  | .invalidVolume => "invalid-volume"
  | .invalidPan => "invalid-pan"
  | .nothingToUndo => "nothing-to-undo"
  | .nothingToRedo => "nothing-to-redo"

def History.render (h : History) : String :=
  let p := h.present
  let tempo := p.tempo.map fun change => s!"{change.tick}:{change.usPerQuarter}"
  let clips := p.clips.map fun c =>
    let notes := c.notes.map fun n => s!"{n.start}/{n.length}/{n.pitch}/{n.velocity}"
    s!"{c.id}:{c.track}:{c.start}:{c.length}[{";".intercalate notes}]"
  let tracks := p.tracks.map fun t =>
    s!"{t.volume}:{t.pan}{if t.muted then "m" else ""}{if t.solo then "s" else ""}"
  s!"tracks={",".intercalate tracks} next={h.nextClipId} tempo={",".intercalate tempo} "
    ++ s!"clips={",".intercalate clips} undo={h.past.length} redo={h.future.length}"

def Event.render : Event → String
  | .off sample track pitch => s!"off:{sample}:{track}:{pitch}"
  | .on sample track pitch velocity => s!"on:{sample}:{track}:{pitch}:{velocity}"

/-- Handle one request, returning the history to continue with and the reply.
A project that breaks the invariant is reported in place of its state. -/
def History.respond (h : History) : Request → History × String
  | .micros tick => (h, s!"micros {h.present.tickToMicros tick}")
  | .schedule rate =>
    (h, s!"schedule {",".intercalate (h.present.schedule rate |>.map Event.render)}")
  | .edit edit =>
    match h.edit edit with
    | .error e => (h, s!"err {e.name}")
    | .ok next =>
      if next.present.wellFormed next.nextClipId then (next, s!"ok {next.render}")
      else (next, s!"invariant-violated {next.render}")

end Pathos
