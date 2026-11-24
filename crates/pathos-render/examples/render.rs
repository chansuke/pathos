//! Renders a short phrase to a WAV file, to listen to the built-in instrument:
//! `cargo run -p pathos-render --example render -- out.wav`.

use std::fs::File;
use std::io::BufWriter;
use std::num::NonZeroU32;

use pathos_core::{Command, Edit, History, PPQ};
use pathos_render::{render, wav};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: render <out.wav>")?;
    let mut history = History::default();
    for command in phrase() {
        history.edit(Edit::Apply(command))?;
    }
    let rate = NonZeroU32::new(48_000).ok_or("a sample rate is not zero")?;
    // Four beats at 120 BPM last two seconds.
    let samples = render(history.present(), rate, 2 * 48_000);
    wav::write(&mut BufWriter::new(File::create(&path)?), rate, &samples)?;
    println!("wrote {path}");
    Ok(())
}

/// C, E and G as eighth notes over a held C below, at 120 BPM. The two
/// tracks add up, so their levels together stay below 1 to keep from clipping.
fn phrase() -> Vec<Command> {
    let eighth = PPQ / 2;
    let mut commands = vec![
        Command::AddTrack,
        Command::AddTrack,
        Command::AddClip {
            track: 0,
            start: 0,
            length: 4 * PPQ,
        },
        Command::AddClip {
            track: 1,
            start: 0,
            length: 4 * PPQ,
        },
        Command::AddNote {
            id: 1,
            start: 0,
            length: 4 * PPQ,
            pitch: 48,
            velocity: 50,
        },
    ];
    for (index, pitch) in [60, 64, 67, 72, 67, 64, 60, 64].into_iter().enumerate() {
        commands.push(Command::AddNote {
            id: 0,
            start: index as u64 * eighth,
            length: eighth,
            pitch,
            velocity: 70,
        });
    }
    commands
}
