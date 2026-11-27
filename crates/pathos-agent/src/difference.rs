//! What a proposal changes, line by line, as the window shows it (ADR 35).

use pathos_core::{CENTER_PAN, Clip, Note, PPQ, Project, Tick, Track};

/// The beats of a bar, as the ruler and the metronome count them.
const BEATS_PER_BAR: u64 = 4;

/// The changes from `before` to `after`: tracks, the master volume, tempo
/// changes, then clips and their notes by id.
pub fn difference(before: &Project, after: &Project) -> Vec<String> {
    let mut changes = Vec::new();
    for index in 0..after.track_count() {
        let new = after.track(index).unwrap_or_default();
        match before.track(index) {
            None => {
                changes.push(format!("add {}", track_name(index)));
                changes.extend(track_changes(index, Track::default(), new));
            }
            Some(old) => changes.extend(track_changes(index, old, new)),
        }
    }
    if before.master_volume() != after.master_volume() {
        changes.push(format!(
            "master volume {} -> {}",
            volume(before.master_volume()),
            volume(after.master_volume())
        ));
    }
    for change in before.tempo() {
        match after.tempo().iter().find(|other| other.tick == change.tick) {
            None => changes.push(format!(
                "remove the tempo change at {}",
                position(change.tick)
            )),
            Some(other) if other.us_per_quarter != change.us_per_quarter => changes.push(format!(
                "tempo at {}: {} -> {}",
                position(change.tick),
                bpm(change.us_per_quarter),
                bpm(other.us_per_quarter)
            )),
            Some(_) => {}
        }
    }
    for change in after.tempo() {
        if !before.tempo().iter().any(|other| other.tick == change.tick) {
            changes.push(format!(
                "tempo {} from {}",
                bpm(change.us_per_quarter),
                position(change.tick)
            ));
        }
    }
    for clip in before.clips() {
        if !after.clips().iter().any(|other| other.id == clip.id) {
            changes.push(format!("delete {}", clip_place(clip)));
        }
    }
    for clip in after.clips() {
        match before.clips().iter().find(|other| other.id == clip.id) {
            None => {
                let notes = after.notes(clip.id).count();
                changes.push(format!(
                    "add {}{}",
                    clip_place(clip),
                    match notes {
                        0 => String::new(),
                        1 => ", with 1 note".into(),
                        notes => format!(", with {notes} notes"),
                    }
                ));
            }
            Some(old) => {
                if (old.track, old.start) != (clip.track, clip.start) {
                    changes.push(format!(
                        "move clip {} to {}, {}",
                        clip.id,
                        track_name(clip.track),
                        position(clip.start)
                    ));
                }
                if old.length != clip.length {
                    changes.push(format!(
                        "clip {} lasts {} instead of {}",
                        clip.id,
                        duration(clip.length),
                        duration(old.length)
                    ));
                }
                let (old_notes, new_notes): (Vec<&Note>, Vec<&Note>) = (
                    before.notes(clip.id).collect(),
                    after.notes(clip.id).collect(),
                );
                for note in &old_notes {
                    if !new_notes.contains(note) {
                        changes.push(format!("clip {}: remove {}", clip.id, note_name(note)));
                    }
                }
                for note in &new_notes {
                    if !old_notes.contains(note) {
                        changes.push(format!("clip {}: add {}", clip.id, note_name(note)));
                    }
                }
            }
        }
    }
    changes
}

fn track_changes(index: u64, old: Track, new: Track) -> Vec<String> {
    let name = track_name(index);
    let mut changes = Vec::new();
    if old.volume != new.volume {
        changes.push(format!(
            "{name} volume {} -> {}",
            volume(old.volume),
            volume(new.volume)
        ));
    }
    if old.pan != new.pan {
        changes.push(format!("{name} pan {} -> {}", pan(old.pan), pan(new.pan)));
    }
    if old.muted != new.muted {
        changes.push(format!(
            "{} {name}",
            if new.muted { "mute" } else { "unmute" }
        ));
    }
    if old.solo != new.solo {
        changes.push(format!(
            "{} {name}",
            if new.solo { "solo" } else { "unsolo" }
        ));
    }
    changes
}

/// A track as the window names it.
fn track_name(index: u64) -> String {
    format!("Track {}", index + 1)
}

fn clip_place(clip: &Clip) -> String {
    format!(
        "clip {} on {}, {}, {}",
        clip.id,
        track_name(clip.track),
        position(clip.start),
        duration(clip.length)
    )
}

/// A tick as a bar and a beat, counted from 1, and the ticks past the beat.
fn position(tick: Tick) -> String {
    let beat = tick / PPQ;
    let past = tick % PPQ;
    let place = format!(
        "bar {} beat {}",
        beat / BEATS_PER_BAR + 1,
        beat % BEATS_PER_BAR + 1
    );
    match past {
        0 => place,
        past => format!("{place} + {past} ticks"),
    }
}

/// A length in beats.
fn duration(ticks: u64) -> String {
    let beats = number(ticks as f64 / PPQ as f64);
    if beats == "1" {
        "1 beat".into()
    } else {
        format!("{beats} beats")
    }
}

/// A number with at most three decimals and no trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn bpm(us_per_quarter: u64) -> String {
    format!("{} BPM", number(60_000_000.0 / us_per_quarter as f64))
}

/// A volume in decibels, as the faders show it.
fn volume(volume: u16) -> String {
    if volume == 0 {
        return "-inf dB".into();
    }
    let decibels = 20.0 * (f64::from(volume) / 1000.0).log10();
    let decibels = (decibels * 10.0).round() / 10.0;
    format!("{:+.1} dB", decibels + 0.0)
}

fn pan(pan: u8) -> String {
    match pan.cmp(&CENTER_PAN) {
        std::cmp::Ordering::Less => format!("L{}", CENTER_PAN - pan),
        std::cmp::Ordering::Equal => "C".into(),
        std::cmp::Ordering::Greater => format!("R{}", pan - CENTER_PAN),
    }
}

/// A note with its pitch as a name, C4 being 60, and where it starts in its
/// clip.
fn note_name(note: &Note) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let pitch = i32::from(note.pitch);
    format!(
        "{}{} at beat {} for {}, velocity {}",
        NAMES[(pitch % 12) as usize],
        pitch / 12 - 1,
        number(note.start as f64 / PPQ as f64 + 1.0),
        duration(note.length),
        note.velocity
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathos_core::{Document, Edit};

    fn project(lines: &[&str]) -> Project {
        let mut document = Document::default();
        for line in lines {
            document.edit(line.parse::<Edit>().unwrap()).unwrap();
        }
        document.history().present().clone()
    }

    #[test]
    fn no_change_is_no_difference() {
        let before = project(&["add-track", "add-clip 0 0 3840"]);
        assert_eq!(difference(&before, &before), Vec::<String>::new());
    }

    #[test]
    fn it_names_tracks_clips_and_notes_as_the_window_does() {
        let before = project(&["add-track", "add-clip 0 0 3840", "add-note 0 0 960 60 100"]);
        let after = project(&[
            "add-track",
            "add-clip 0 0 3840",
            "add-note 0 0 960 60 100",
            "add-track",
            "set-track-volume 1 500",
            "set-track-pan 1 60",
            "mute-track 0",
            "set-master-volume 0",
            "set-tempo 0 400000",
            "set-tempo 4080 600000",
            "remove-note 0 0 60",
            "add-note 0 1440 480 61 90",
            "move-clip 0 1 1920",
            "add-clip 0 960 1920",
            "add-note 1 0 240 72 100",
        ]);
        assert_eq!(
            difference(&before, &after),
            [
                "mute Track 1",
                "add Track 2",
                "Track 2 volume +0.0 dB -> -6.0 dB",
                "Track 2 pan C -> L40",
                "master volume +0.0 dB -> -inf dB",
                "tempo at bar 1 beat 1: 120 BPM -> 150 BPM",
                "tempo 100 BPM from bar 2 beat 1 + 240 ticks",
                "move clip 0 to Track 2, bar 1 beat 3",
                "clip 0: remove C4 at beat 1 for 1 beat, velocity 100",
                "clip 0: add C#4 at beat 2.5 for 0.5 beats, velocity 90",
                "add clip 1 on Track 1, bar 1 beat 2, 2 beats, with 1 note",
            ]
        );
    }

    #[test]
    fn it_shows_a_split_and_a_deleted_clip() {
        let before = project(&["add-track", "add-clip 0 0 3840", "add-clip 0 3840 960"]);
        let after = project(&[
            "add-track",
            "add-clip 0 0 3840",
            "add-clip 0 3840 960",
            "split-clip 0 1920",
            "delete-clip 1",
        ]);
        assert_eq!(
            difference(&before, &after),
            [
                "delete clip 1 on Track 1, bar 2 beat 1, 1 beat",
                "clip 0 lasts 2 beats instead of 4 beats",
                "add clip 2 on Track 1, bar 1 beat 3, 2 beats",
            ]
        );
    }

    #[test]
    fn it_shows_unmute_solo_unsolo_right_pan_removed_tempo_and_multiple_notes() {
        let before = project(&[
            "add-track",
            "add-track",
            "mute-track 0",
            "solo-track 1",
            "set-tempo 3840 600000",
        ]);
        let after = project(&[
            "add-track",
            "add-track",
            "mute-track 0",
            "solo-track 1",
            "set-tempo 3840 600000",
            "unmute-track 0",
            "solo-track 0",
            "set-track-pan 0 140",
            "unsolo-track 1",
            "remove-tempo 3840",
            "add-clip 0 0 3840",
            "add-note 0 0 960 60 100",
            "add-note 0 960 960 64 100",
        ]);
        assert_eq!(
            difference(&before, &after),
            [
                "Track 1 pan C -> R40",
                "unmute Track 1",
                "solo Track 1",
                "unsolo Track 2",
                "remove the tempo change at bar 2 beat 1",
                "add clip 0 on Track 1, bar 1 beat 1, 4 beats, with 2 notes",
            ]
        );
    }
}
