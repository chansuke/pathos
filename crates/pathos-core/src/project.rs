use std::fmt;

/// Musical time in ticks.
pub type Tick = u64;

/// Ticks per quarter note.
pub const PPQ: u64 = 960;

/// No clip or tempo change may lie beyond this tick.
pub const MAX_TICK: Tick = 1 << 40;

/// Slowest accepted tempo, in microseconds per quarter note (1 BPM).
pub const MAX_US_PER_QUARTER: u64 = 60_000_000;

/// From `tick` onwards a quarter note lasts `us_per_quarter` microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TempoChange {
    pub tick: Tick,
    pub us_per_quarter: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub id: u64,
    pub track: u64,
    pub start: Tick,
    pub length: u64,
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
            EditError::NothingToUndo => "nothing to undo",
            EditError::NothingToRedo => "nothing to redo",
        })
    }
}

impl std::error::Error for EditError {}

/// A project. The only way to obtain a different one is [`Project::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    track_count: u64,
    next_clip_id: u64,
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
            next_clip_id: 0,
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

    /// The id the next created clip receives. Ids are never reused.
    pub fn next_clip_id(&self) -> u64 {
        self.next_clip_id
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

    /// Apply a command, returning the resulting project. Checks run in the
    /// order the model lists them, which fixes the error reported when several
    /// apply.
    pub fn apply(&self, command: Command) -> Result<Project, EditError> {
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
                    id: self.next_clip_id,
                    track,
                    start,
                    length,
                };
                if self.collides(&clip) {
                    return Err(EditError::Overlap);
                }
                next.clips.push(clip);
                next.next_clip_id += 1;
            }
            Command::MoveClip { id, track, start } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                if track >= self.track_count {
                    return Err(EditError::UnknownTrack);
                }
                let clip = self.clips[index];
                if exceeds_max_tick(start, clip.length) {
                    return Err(EditError::OutOfRange);
                }
                let moved = Clip {
                    track,
                    start,
                    ..clip
                };
                if self.collides(&moved) {
                    return Err(EditError::Overlap);
                }
                next.clips[index] = moved;
            }
            Command::SplitClip { id, tick } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                let clip = self.clips[index];
                if tick <= clip.start || tick >= clip.stop() {
                    return Err(EditError::SplitOutsideClip);
                }
                next.clips[index].length = tick - clip.start;
                next.clips.push(Clip {
                    id: self.next_clip_id,
                    track: clip.track,
                    start: tick,
                    length: clip.stop() - tick,
                });
                next.next_clip_id += 1;
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
        }
        Ok(next)
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
