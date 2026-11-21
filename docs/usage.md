# Usage

The window shows a bar of transport controls at the top, a timeline of tracks
and MIDI clips under a ruler, a piano roll for the selected clip above the
bottom, and the keys or the latest message at the bottom. Use `Cmd` instead of `Ctrl` on macOS.

## Editing

- `T` adds a track, and a double click on a lane adds a one-bar clip.
- The mouse wheel or a trackpad over the tracks scrolls them when they do not
  all fit. A new track scrolls into view.
- Dragging a clip moves it by whole beats, also to another track.
- `S` splits the selected clip at the beat under the pointer, and `Delete`
  deletes it.
- A click in the piano roll adds a sixteenth note, and a click on a note
  removes it.
- The piano roll shows a keyboard from C2 to C6 at its left and numbers the
  beats of the clip above it; its notes take the color of the clip's track.
- `Ctrl-Z` and `Ctrl-Shift-Z` undo and redo.

## Mixing

- Each track has a color, shown at the left of its header and on its clips. The
  clips of a muted track are fainter.
- `M` in a track's header mutes it and `S` solos it.
- Dragging the volume up or down changes it by a quarter decibel a pixel, and
  dragging the pan (`C`, `L` or `R`) left or right moves the track between the
  channels. A double click returns either to its default. Both are heard while
  they are dragged.
- `Master` at the right of the top bar is the volume of the whole song,
  dragged and reset like a track's volume. It does not change the metronome.
- The two bars beside it meter the left and right channel of the song during
  playback, from -60 dB to +6 dB: green up to -6 dB, yellow up to full scale
  and red past it. A mark on each bar holds its latest peak for two seconds.

## Playback

- `Space` or the play button plays and stops, and `Home` or the button before
  it returns to the start. Playback matches the offline render sample for
  sample.
- The top bar shows where the playhead is, as bar, beat and sixteenth, the
  time from the start, and the tempo there.
- `L` or the `Loop` button loops the selected clip, or every clip when none is
  selected, and stops looping when a loop is on. The ruler marks the loop. The
  loop is not saved in the project.
- `K` or the `Metronome` button turns the metronome on and off. It clicks on every beat during playback,
  higher on the first beat of each bar, and is not in a rendered file.

## Files

- `Ctrl-S` saves and `Ctrl-Shift-S` saves under a new name; `Ctrl-O` opens a
  file.
- `cargo run -- song.pathos` opens `song.pathos`, or creates it on the first
  save.
- A project file lists the edits that built the project, one per line, so undo
  still works after reopening it.
- Closing the window with unsaved changes asks whether to save them first.

## Agents

An agent, such as a coding assistant, can read the open project and propose
edits to it through `pathos-mcp`, an MCP server (ADR 5, ADR 35). Build it and
register it with the agent, for example with Claude Code:

```sh
nix build
claude mcp add pathos -- "$PWD/result/bin/pathos-mcp"
```

- The agent's tools are `get_project`, `propose_commands` and `get_proposal`.
  It reaches the window that is open; only one window at a time answers.
- An agent only proposes. A proposal appears at the top right with its summary
  and what it changes. `Enter` or `Apply` applies it, and `Escape` or
  `Dismiss` dismisses it. Undo takes its commands back one at a time.
- A proposal with a rejected command is refused, and the agent is told which.
  A proposal goes stale when the project changes before it is applied.
- The window listens on `$XDG_RUNTIME_DIR/pathos/agent.sock`, or on
  `$PATHOS_SOCKET` when it is set, for the window and `pathos-mcp` alike.
