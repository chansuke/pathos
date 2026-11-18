use std::fmt;
use std::ops::Range;

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

/// The loudest track volume, a gain of 2 (ADR 20).
pub const MAX_VOLUME: u16 = 2000;

/// The volume of a new track, a gain of 1.
pub const UNITY_VOLUME: u16 = 1000;

/// The rightmost pan; 0 is leftmost (ADR 21).
pub const MAX_PAN: u8 = 200;

/// The pan of a new track.
pub const CENTER_PAN: u8 = 100;

/// A track's volume is its gain in thousandths (ADR 20). A muted track is not
/// heard, and when any track is soloed, only soloed tracks are (ADR 21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Track {
    pub volume: u16,
    pub muted: bool,
    pub solo: bool,
    pub pan: u8,
}

impl Default for Track {
    /// A new track.
    fn default() -> Self {
        Track {
            volume: UNITY_VOLUME,
            muted: false,
            solo: false,
            pan: CENTER_PAN,
        }
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
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
    SetTrackVolume {
        track: u64,
        volume: u64,
    },
    SetTrackMute {
        track: u64,
        muted: bool,
    },
    SetTrackSolo {
        track: u64,
        solo: bool,
    },
    SetTrackPan {
        track: u64,
        pan: u64,
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
    InvalidVolume,
    InvalidPan,
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
            EditError::InvalidVolume => "volume out of range",
            EditError::InvalidPan => "pan out of range",
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
    /// The tracks whose volume or mute differs from a new track's, with their
    /// index, ordered by index. The model keeps every track in a list; keeping
    /// the count a plain number lets the Kani harnesses try any count.
    mixed: Vec<(u64, Track)>,
    /// Ordered by tick, starting at tick 0.
    tempo: Vec<TempoChange>,
    /// Ordered by id.
    clips: Vec<Clip>,
    /// The notes of every clip with the id of their clip, ordered by clip id,
    /// then start, then pitch. The model keeps notes inside their clip; one
    /// list keeps `Clip` a plain value, which the Kani harnesses rely on.
    notes: Vec<(u64, Note)>,
}

impl Default for Project {
    /// An empty project at 120 BPM.
    fn default() -> Self {
        Project {
            track_count: 0,
            mixed: Vec::new(),
            tempo: vec![TempoChange {
                tick: 0,
                us_per_quarter: 500_000,
            }],
            clips: Vec::new(),
            notes: Vec::new(),
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

    /// The volume and mute of `track`, if the project has it.
    pub fn track(&self, track: u64) -> Option<Track> {
        (track < self.track_count).then(|| match self.mixed_index(track) {
            Ok(at) => self.mixed[at].1,
            Err(_) => Track::default(),
        })
    }

    fn mixed_index(&self, track: u64) -> Result<usize, usize> {
        self.mixed.binary_search_by_key(&track, |(index, _)| *index)
    }

    /// Whether `track` is heard: it is not muted, and it is soloed if any
    /// track is (ADR 21).
    pub fn heard(&self, track: u64) -> bool {
        let any_solo = self.mixed.iter().any(|(_, track)| track.solo);
        self.track(track)
            .is_none_or(|track| !track.muted && (track.solo || !any_solo))
    }

    /// Gives `track` the settings of `settings`, keeping only tracks
    /// that differ from a new one in `mixed`.
    fn set_track(&mut self, track: u64, settings: Track) {
        match (self.mixed_index(track), settings == Track::default()) {
            (Ok(at), true) => {
                self.mixed.remove(at);
            }
            (Ok(at), false) => self.mixed[at].1 = settings,
            (Err(_), true) => {}
            (Err(at), false) => self.mixed.insert(at, (track, settings)),
        }
    }

    pub fn tempo(&self) -> &[TempoChange] {
        &self.tempo
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// The notes of the clip `id`, ordered by start, then pitch.
    pub fn notes(&self, id: u64) -> impl Iterator<Item = &Note> {
        self.notes[self.note_range(id)].iter().map(|(_, note)| note)
    }

    /// Where the notes of the clip `id` lie in `self.notes`. The searches
    /// here and in `note_index` are linear: an insertion moves the notes after
    /// it anyway, and a binary search made the Kani harnesses of note commands
    /// run past their time limit (ADR 16).
    fn note_range(&self, id: u64) -> Range<usize> {
        let start = self
            .notes
            .iter()
            .position(|(clip, _)| *clip >= id)
            .unwrap_or(self.notes.len());
        let count = self.notes[start..]
            .iter()
            .take_while(|(clip, _)| *clip == id)
            .count();
        start..start + count
    }

    /// The index of the note of the clip `id` with `key`, or where it would be
    /// inserted.
    fn note_index(&self, id: u64, key: (Tick, u64)) -> Result<usize, usize> {
        let wanted = (id, key);
        match self
            .notes
            .iter()
            .position(|(clip, note)| (*clip, note.key()) >= wanted)
        {
            Some(at) if (self.notes[at].0, self.notes[at].1.key()) == wanted => Ok(at),
            Some(at) => Err(at),
            None => Err(self.notes.len()),
        }
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
                    id: next_clip_id,
                    track: clip.track,
                    start: tick,
                    length: clip.stop() - tick,
                });
                // A note that starts before the split stays in the left clip
                // and ends at the split at the latest (ADR 14).
                let offset = tick - clip.start;
                let range = self.note_range(id);
                let split = range.start
                    + self.notes[range.clone()].partition_point(|(_, note)| note.start < offset);
                for (_, note) in &mut next.notes[range.start..split] {
                    note.length = note.length.min(offset - note.start);
                }
                // The new clip has the highest id, so its notes come last.
                let moved = range.end - split;
                next.notes[split..].rotate_left(moved);
                let tail = next.notes.len() - moved;
                for (clip, note) in &mut next.notes[tail..] {
                    *clip = next_clip_id;
                    note.start -= offset;
                }
                next_clip_id += 1;
            }
            Command::DeleteClip { id } => {
                let index = self.clip_index(id).ok_or(EditError::UnknownClip)?;
                next.clips.remove(index);
                next.notes.drain(self.note_range(id));
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
                if start
                    .checked_add(length)
                    .is_none_or(|stop| stop > self.clips[index].length)
                {
                    return Err(EditError::NoteOutsideClip);
                }
                if self.notes(id).any(|other| note.overlaps(other)) {
                    return Err(EditError::NoteOverlap);
                }
                // No note has the key of the new one, which would overlap it.
                let (Ok(at) | Err(at)) = self.note_index(id, note.key());
                next.notes.insert(at, (id, note));
            }
            Command::RemoveNote { id, start, pitch } => {
                self.clip_index(id).ok_or(EditError::UnknownClip)?;
                let at = self
                    .note_index(id, (start, pitch))
                    .map_err(|_| EditError::UnknownNote)?;
                next.notes.remove(at);
            }
            Command::SetTrackVolume { track, volume } => {
                let settings = self.track(track).ok_or(EditError::UnknownTrack)?;
                let volume = u16::try_from(volume)
                    .ok()
                    .filter(|volume| *volume <= MAX_VOLUME)
                    .ok_or(EditError::InvalidVolume)?;
                next.set_track(track, Track { volume, ..settings });
            }
            Command::SetTrackMute { track, muted } => {
                let settings = self.track(track).ok_or(EditError::UnknownTrack)?;
                next.set_track(track, Track { muted, ..settings });
            }
            Command::SetTrackSolo { track, solo } => {
                let settings = self.track(track).ok_or(EditError::UnknownTrack)?;
                next.set_track(track, Track { solo, ..settings });
            }
            Command::SetTrackPan { track, pan } => {
                let settings = self.track(track).ok_or(EditError::UnknownTrack)?;
                let pan = u8::try_from(pan)
                    .ok()
                    .filter(|pan| *pan <= MAX_PAN)
                    .ok_or(EditError::InvalidPan)?;
                next.set_track(track, Track { pan, ..settings });
            }
        }
        Ok((next, next_clip_id))
    }

    /// The time of `tick` in microseconds from the start, rounded down.
    pub fn tick_to_micros(&self, tick: Tick) -> u128 {
        micros_times_ppq(&self.tempo, tick) / u128::from(PPQ)
    }
}

/// [`PPQ`] times the microseconds from the start to `tick` under `tempo`, a
/// project's tempo changes.
pub(crate) fn micros_times_ppq(tempo: &[TempoChange], tick: Tick) -> u128 {
    let mut micros_times_ppq: u128 = 0;
    for (index, change) in tempo.iter().enumerate() {
        // Each tempo applies up to the next change, the last one forever.
        let end = match tempo.get(index + 1) {
            Some(next) if tick > next.tick => next.tick,
            _ => tick,
        };
        micros_times_ppq +=
            u128::from(end.saturating_sub(change.tick)) * u128::from(change.us_per_quarter);
        if end == tick {
            break;
        }
    }
    micros_times_ppq
}
