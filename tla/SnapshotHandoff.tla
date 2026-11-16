---------------------------- MODULE SnapshotHandoff ----------------------------
(***************************************************************************)
(* How the UI thread hands snapshots of the project to the audio thread    *)
(* during playback (ADR 18). The audio thread must never wait, allocate or *)
(* free, so it only takes a snapshot from a slot and gives the one it      *)
(* played back through a bounded queue, and the UI thread frees it there.  *)
(*                                                                         *)
(* A snapshot is an opaque number: what it holds is the Lean model's       *)
(* (ADR 17). Snapshots are numbered in the order the UI publishes them.    *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS
    Snapshots,  \* How many snapshots the UI publishes in a behavior.
    Capacity    \* The length of the queue of retired snapshots.

ASSUME Snapshots \in Nat /\ Capacity \in Nat \ {0}

None == 0
Snapshot == 1..Snapshots

VARIABLES
    published,  \* The last snapshot the UI published.
    slot,       \* The snapshot waiting for the audio thread, or None.
    playing,    \* The snapshot the audio thread plays, or None.
    retired,    \* The queue from the audio thread back to the UI thread.
    freed,      \* The snapshots the UI thread has freed.
    audio,      \* Where the audio thread is in taking a snapshot.
    taken       \* The snapshot the audio thread has swapped out, or None.

vars == <<published, slot, playing, retired, freed, audio, taken>>

Init ==
    /\ published = None
    /\ slot = None
    /\ playing = None
    /\ retired = <<>>
    /\ freed = {}
    /\ audio = "idle"
    /\ taken = None

(* The UI publishes a new snapshot with one atomic swap. A snapshot it    *)
(* takes back from the slot was never seen by the audio thread, so the UI *)
(* frees it at once.                                                      *)
Publish ==
    /\ published < Snapshots
    /\ published' = published + 1
    /\ slot' = published + 1
    /\ freed' = IF slot = None THEN freed ELSE freed \union {slot}
    /\ UNCHANGED <<playing, retired, audio, taken>>

(* At the start of a block, the audio thread checks that the queue has   *)
(* room, swaps the waiting snapshot out of the slot, and queues the one   *)
(* it played. Each is a separate step. The UI only removes from the       *)
(* queue, so the room it saw is still there when it pushes.               *)
CheckRoom ==
    /\ audio = "idle"
    /\ Len(retired) < Capacity
    /\ audio' = "swap"
    /\ UNCHANGED <<published, slot, playing, retired, freed, taken>>

Swap ==
    /\ audio = "swap"
    /\ taken' = slot
    /\ slot' = None
    /\ audio' = IF slot = None THEN "idle" ELSE "push"
    /\ UNCHANGED <<published, playing, retired, freed>>

Push ==
    /\ audio = "push"
    /\ retired' = IF playing = None THEN retired ELSE Append(retired, playing)
    /\ playing' = taken
    /\ taken' = None
    /\ audio' = "idle"
    /\ UNCHANGED <<published, slot, freed>>

Take == CheckRoom \/ Swap \/ Push

(* The UI frees the oldest retired snapshot. *)
Free ==
    /\ retired # <<>>
    /\ freed' = freed \union {Head(retired)}
    /\ retired' = Tail(retired)
    /\ UNCHANGED <<published, slot, playing, audio, taken>>

Next == Publish \/ Take \/ Free

(* The audio thread runs a block now and then, and the UI drains the      *)
(* queue now and then. CheckRoom needs only weak fairness once the slot   *)
(* holds a snapshot, since the queue then drains.                          *)
Fairness ==
    /\ WF_vars(slot # None /\ CheckRoom)
    /\ WF_vars(Swap) /\ WF_vars(Push) /\ WF_vars(Free)

Spec == Init /\ [][Next]_vars /\ Fairness

-------------------------------------------------------------------------------
TypeOK ==
    /\ published \in 0..Snapshots
    /\ slot \in {None} \union Snapshot
    /\ playing \in {None} \union Snapshot
    /\ retired \in Seq(Snapshot)
    /\ Len(retired) <= Capacity
    /\ freed \subseteq Snapshot
    /\ audio \in {"idle", "swap", "push"}
    /\ taken \in {None} \union Snapshot

Range(s) == {s[i] : i \in DOMAIN s}

(* No snapshot is used after it is freed: what the audio thread plays,   *)
(* what waits for it and what waits for the UI are all alive.            *)
NoUseAfterFree ==
    /\ playing \notin freed
    /\ slot \notin freed
    /\ taken \notin freed
    /\ Range(retired) \intersect freed = {}

(* Every published snapshot is in exactly one place, so none leaks and   *)
(* none is freed twice.                                                  *)
EachSnapshotOnce ==
    \A s \in 1..published :
        Cardinality({w \in {"slot", "taken", "playing", "freed"} :
            \/ w = "slot" /\ slot = s
            \/ w = "taken" /\ taken = s
            \/ w = "playing" /\ playing = s
            \/ w = "freed" /\ s \in freed})
          + Cardinality({i \in DOMAIN retired : retired[i] = s}) = 1

(* The audio thread plays newer snapshots over time. *)
PlaysInOrder == [][playing' = playing \/ playing' > playing]_vars

(* Once the UI stops publishing, the audio thread plays the last snapshot *)
(* and every other one is freed.                                           *)
Settles ==
    <>[](published = Snapshots) ~>
        (playing = Snapshots /\ freed = Snapshot \ {Snapshots})
================================================================================
