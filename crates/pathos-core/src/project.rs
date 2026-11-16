use std::fmt;

#[cfg(kani)]
mod verification;

/// Musical time in ticks.
pub type Tick = u64;

/// Ticks per quarter note.
pub const PPQ: u64 = 960;

/// No clip or tempo change may lie beyond this tick.
pub const MAX_TICK: Tick = 1 << 40;

/// Slowest accepted tempo, in microseconds per quarter note (1 BPM).
pub const MAX_US_PER_QUARTER: u64 = 60_000_000;

/// The highest MIDI pitch.
pub const MAX_PITCH: u8 = 127;

/// The highest MIDI velocity. Velocity 0 means note-off in MIDI, so a note's
/// velocity is at least 1.
pub const MAX_VELOCITY: u8 = 127;

/// From `tick` onwards a quarter note lasts `us_per_quarter` microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct TempoChange {
    pub tick: Tick,
    pub us_per_quarter: u64,
}

/// A MIDI note, starting `start` ticks after the start of its clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Note {
    pub start: Tick,
    pub length: u64,
    pub pitch: u8,
    pub velocity: u8,
}

impl Note {
    /// The tick just past the end of the note, relative to its clip.
    pub fn stop(&self) -> Tick {
        self.start + self.length
    }

    fn overlaps(&self, other: &Note) -> bool {
        self.pitch == other.pitch && self.start < other.stop() && other.start < self.stop()
    }

    /// The order of notes in a clip: by start, then pitch.
    fn key(&self) -> (Tick, u64) {
        (self.start, u64::from(self.pitch))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub id: u64,
    pub track: u64,
    pub start: Tick,
    pub length: u64,
    /// Ordered by start, then pitch.
    pub notes: Vec<Note>,
}

impl Clip {
    /// The tick just past the end of the clip.
    pub fn stop(&self) -> Tick {
        self.start + self.length
    }

    fn overlaps(&self, other: &Clip) -> bool {
        self.track == other.track && self.start < other.stop() && other.start < self.stop()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    AddTrack,
    AddClip {
        track: u64,
        start: Tick,
        length: u64,
    },
    /// Move a clip to `track`, starting at `start`.
    MoveClip {
        id: u64,
        track: u64,
        start: Tick,
    },
    /// Split a clip in two at `tick`, which must lie strictly inside it.
    SplitClip {
        id: u64,
        tick: Tick,
    },
    DeleteClip {
        id: u64,
    },
    /// Set the tempo from `tick` onwards, replacing a change already there.
    SetTempo {
        tick: Tick,
        us_per_quarter: u64,
    },
    RemoveTempo {
        tick: Tick,
    },
    /// Add a note to a clip, `start` ticks after the start of the clip.
    AddNote {
        id: u64,
        start: Tick,
        length: u64,
        pitch: u64,
        velocity: u64,
    },
    /// Remove the note of `pitch` that starts `start` ticks into a clip.
    RemoveNote {
        id: u64,
        start: Tick,
        pitch: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    EmptyClip,
    UnknownTrack,
    UnknownClip,
    OutOfRange,
    Overlap,
    SplitOutsideClip,
    InvalidTempo,
    InitialTempo,
    NoTempoChange,
    EmptyNote,
    InvalidNote,
    NoteOutsideClip,
    NoteOverlap,
    UnknownNote,
    NothingToUndo,
    NothingToRedo,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EditError::EmptyClip => "a clip must not be empty",
            EditError::UnknownTrack => "no such track",
            EditError::UnknownClip => "no such clip",
            EditError::OutOfRange => "beyond the last supported tick",
            EditError::Overlap => "clips on a track must not overlap",
            EditError::SplitOutsideClip => "the split point is not inside the clip",
            EditError::InvalidTempo => "tempo out of range",
            EditError::InitialTempo => "the tempo at tick 0 cannot be removed",
            EditError::NoTempoChange => "no tempo change at that tick",
            EditError::EmptyNote => "a note must not be empty",
            EditError::InvalidNote => "pitch or velocity out of range",
            EditError::NoteOutsideClip => "a note must end within its clip",
            EditError::NoteOverlap => "notes of one pitch must not overlap",
            EditError::UnknownNote => "no such note",
            EditError::NothingToUndo => "nothing to undo",
            EditError::NothingToRedo => "nothing to redo",
        })
    }
}

impl std::error::Error for EditError {}

/// A project. The only way to obtain a different one is [`Project::apply`].
///
/// The id the next created clip receives is not part of the project but of the
/// [`History`](crate::History), so that undo does not reissue ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    track_count: u64,
    /// Ordered by tick, starting at tick 0.
    tempo: Vec<TempoChange>,
    /// Ordered by id.
    clips: Vec<Clip>,
}

impl Default for Project {
    /// An empty project at 120 BPM.
    fn default() -> Self {
        Project {
            track_count: 0,
            tempo: vec![TempoChange {
                tick: 0,
                us_per_quarter: 500_000,
            }],
            clips: Vec::new(),
        }
    }
}

/// Whether a clip of `length` starting at `start` ends beyond [`MAX_TICK`].
fn exceeds_max_tick(start: Tick, length: u64) -> bool {
    start.checked_add(length).is_none_or(|stop| stop > MAX_TICK)
}

impl Project {
    pub fn track_count(&self) -> u64 {
        self.track_count
    }

    pub fn tempo(&self) -> &[TempoChange] {
        &self.tempo
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    fn clip_index(&self, id: u64) -> Option<usize> {
        self.clips.binary_search_by_key(&id, |clip| clip.id).ok()
    }

    /// Whether `clip` overlaps a clip of the project other than itself.
    fn collides(&self, clip: &Clip) -> bool {
        self.clips
            .iter()
            .any(|other| other.id != clip.id && clip.overlaps(other))
    }

    /// Apply a command, where `next_clip_id` is the id the next created clip
    /// receives. Returns the resulting project and the id after the command.
    /// Checks run in the order the model lists them, which fixes the error
    /// reported when several apply.
    pub fn apply(
        &self,
        mut next_clip_id: u64,
        command: Command,
    ) -> Result<(Project, u64), EditError> {
        let mut next = self.clone();
        match command {
            Command::AddTrack => next.track_count += 1,
            Command::AddClip {
                track,
                start,
                length,
            } => {
                if length == 0 {
                    return Err(EditError::EmptyClip);
                }
                if track >= self.track_count {
                    return Err(EditError::UnknownTrack);
                }
                if exceeds_max_tick(start, length) {
                    return Err(EditError::OutOfRange);
                }
                let clip = Clip {
                    id: next_clip_id,
                    track,
                    start,
                    length,
                    notes: Vec::new(),
                };
                if self.collides(&clip) {
                    return Err(EditError::Overlap);
                }
                next.clips.push(clip);
                next_clip_id += 1;
            }
            Command::MoveClip { id, track, start } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                if track >= self.track_count {
                    return Err(EditError::UnknownTrack);
                }
                let clip = &self.clips[index];
                if exceeds_max_tick(start, clip.length) {
                    return Err(EditError::OutOfRange);
                }
                let moved = Clip {
                    track,
                    start,
                    ..clip.clone()
                };
                if self.collides(&moved) {
                    return Err(EditError::Overlap);
                }
                next.clips[index] = moved;
            }
            Command::SplitClip { id, tick } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                let clip = &self.clips[index];
                if tick <= clip.start || tick >= clip.stop() {
                    return Err(EditError::SplitOutsideClip);
                }
                // A note that starts before the split stays in the left clip
                // and ends at the split at the latest (ADR 14).
                let offset = tick - clip.start;
                let (left, right): (Vec<Note>, Vec<Note>) =
                    clip.notes.iter().partition(|note| note.start < offset);
                let right = Clip {
                    id: next_clip_id,
                    track: clip.track,
                    start: tick,
                    length: clip.stop() - tick,
                    notes: right
                        .into_iter()
                        .map(|note| Note {
                            start: note.start - offset,
                            ..note
                        })
                        .collect(),
                };
                next.clips[index] = Clip {
                    length: offset,
                    notes: left
                        .into_iter()
                        .map(|note| Note {
                            length: note.length.min(offset - note.start),
                            ..note
                        })
                        .collect(),
                    ..clip.clone()
                };
                next.clips.push(right);
                next_clip_id += 1;
            }
            Command::DeleteClip { id } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                next.clips.remove(index);
            }
            Command::SetTempo {
                tick,
                us_per_quarter,
            } => {
                if us_per_quarter == 0 || us_per_quarter > MAX_US_PER_QUARTER {
                    return Err(EditError::InvalidTempo);
                }
                if tick > MAX_TICK {
                    return Err(EditError::OutOfRange);
                }
                let change = TempoChange {
                    tick,
                    us_per_quarter,
                };
                match self.tempo.binary_search_by_key(&tick, |change| change.tick) {
                    Ok(index) => next.tempo[index] = change,
                    Err(index) => next.tempo.insert(index, change),
                }
            }
            Command::RemoveTempo { tick } => {
                if tick == 0 {
                    return Err(EditError::InitialTempo);
                }
                let index = self
                    .tempo
                    .binary_search_by_key(&tick, |change| change.tick)
                    .map_err(|_| EditError::NoTempoChange)?;
                next.tempo.remove(index);
            }
            Command::AddNote {
                id,
                start,
                length,
                pitch,
                velocity,
            } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                if length == 0 {
                    return Err(EditError::EmptyNote);
                }
                let note = match (u8::try_from(pitch), u8::try_from(velocity)) {
                    (Ok(pitch), Ok(velocity))
                        if pitch <= MAX_PITCH && (1..=MAX_VELOCITY).contains(&velocity) =>
                    {
                        Note {
                            start,
                            length,
                            pitch,
                            velocity,
                        }
                    }
                    _ => return Err(EditError::InvalidNote),
                };
                let clip = &self.clips[index];
                if start
                    .checked_add(length)
                    .is_none_or(|stop| stop > clip.length)
                {
                    return Err(EditError::NoteOutsideClip);
                }
                if clip.notes.iter().any(|other| note.overlaps(other)) {
                    return Err(EditError::NoteOverlap);
                }
                let notes = &mut next.clips[index].notes;
                // No note has the key of the new one, which would overlap it.
                let (Ok(at) | Err(at)) = notes.binary_search_by_key(&note.key(), Note::key);
                notes.insert(at, note);
            }
            Command::RemoveNote { id, start, pitch } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                let notes = &mut next.clips[index].notes;
                let at = notes
                    .binary_search_by_key(&(start, pitch), Note::key)
                    .map_err(|_| EditError::UnknownNote)?;
                notes.remove(at);
            }
        }
        Ok((next, next_clip_id))
    }

    /// The time of `tick` in microseconds from the start, rounded down.
    pub fn tick_to_micros(&self, tick: Tick) -> u128 {
        let mut micros_times_ppq: u128 = 0;
        for (index, change) in self.tempo.iter().enumerate() {
            // Each tempo applies up to the next change, the last one forever.
            let end = match self.tempo.get(index + 1) {
                Some(next) if tick > next.tick => next.tick,
                _ => tick,
            };
            micros_times_ppq +=
                u128::from(end.saturating_sub(change.tick)) * u128::from(change.us_per_quarter);
            if end == tick {
                break;
            }
        }
        micros_times_ppq / u128::from(PPQ)
    }
}
