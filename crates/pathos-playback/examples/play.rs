//! Plays a short phrase on the default output device, to check playback by
//! ear: `cargo run -p pathos-playback --example play`.

use std::time::Duration;

use pathos_core::{Command, Edit, History, PPQ};
use pathos_playback::output::Output;
use pathos_render::Score;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut history = History::default();
    history.edit(Edit::Apply(Command::AddTrack))?;
    history.edit(Edit::Apply(Command::AddClip {
        track: 0,
        start: 0,
        length: 4 * PPQ,
    }))?;
    for (index, pitch) in [60, 64, 67, 72, 67, 64, 60, 64].into_iter().enumerate() {
        history.edit(Edit::Apply(Command::AddNote {
            id: 0,
            start: index as u64 * PPQ / 2,
            length: PPQ / 2,
            pitch,
            velocity: 90,
        }))?;
    }
    let output = Output::open()?;
    println!("playing at {} Hz", output.sample_rate());
    output.publish(Score::new(history.present(), output.sample_rate()));
    output.transport().play();
    std::thread::sleep(Duration::from_millis(2200));
    println!("reached frame {}", output.transport().position());
    Ok(())
}
