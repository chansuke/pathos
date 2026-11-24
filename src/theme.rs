//! The colors and the font of the window, and the key caps that the footer,
//! the toggles and the proposals of agents share.

use gpui::{Div, div, prelude::*, px, rgb};

pub const BACKGROUND: u32 = 0x131518;
/// The ground of the track headers.
pub const PANEL: u32 = 0x1a1d21;
pub const LANE: u32 = 0x17191d;
pub const BLACK_KEY_ROW: u32 = 0x16181b;
pub const WHITE_KEY_ROW: u32 = 0x1d2024;
pub const WHITE_KEY: u32 = 0xd4d8de;
/// The line between two white keys; the one below a C or an F is darker.
pub const WHITE_KEY_LINE: u32 = 0xc3c8cf;
pub const BLACK_KEY: u32 = 0x23272c;
pub const GRID: u32 = 0x2a2e34;
/// The line of each beat, fainter than the line of a bar.
pub const BEAT_LINE: u32 = 0x1e2125;
pub const CONTROL: u32 = 0x202328;
pub const CONTROL_BORDER: u32 = 0x343941;
pub const TEXT: u32 = 0xe6e8eb;
/// Labels and values that are read less often than names.
pub const SUBTLE_TEXT: u32 = 0x8a919b;
/// Text on controls that are off.
pub const SOFT_TEXT: u32 = 0xc9ced6;
pub const SELECTION: u32 = 0xf2f4f7;
pub const ERROR: u32 = 0xe06c75;
pub const PLAYHEAD: u32 = 0xf0f0f0;
pub const MUTED: u32 = 0xffb066;
pub const SOLOED: u32 = 0x67c587;
/// The colors of the tracks in turn. A track's color follows its place, so it
/// is not part of the project.
pub const TRACK_COLORS: [u32; 6] = [0x5aa9ff, 0xb48cff, 0x4fd1c5, 0xf48fb1, 0xe2b54a, 0x67c587];
/// The color of the play button, the loop and the toggles that are on.
pub const ACCENT: u32 = 0x5aa9ff;
/// The loop range over the lanes, translucent so that clips show through.
pub const LOOP: u32 = ACCENT << 8 | 0x12;
/// The ground of a readout, darker than the bar around it.
pub const READOUT: u32 = 0x0f1113;
/// The range a finding of a review points at (ADR 36).
pub const FINDING: u32 = 0xe2b54a;
/// The meter's zones: up to -6 dB, up to full scale, and past it.
pub const METER_LOW: u32 = 0x57b97a;
pub const METER_HIGH: u32 = 0xe2b54a;
/// The bundled font of the numbers and the keys, whose digits keep one width
/// (ADR 31).
pub const MONO: &str = "JetBrains Mono";

/// A key as the footer and the toggles show it.
pub fn key_cap(key: &'static str) -> Div {
    div()
        .px(px(5.0))
        .rounded(px(4.0))
        .border_1()
        .border_b_2()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(CONTROL))
        .font_family(MONO)
        .text_size(px(11.0))
        .text_color(rgb(SOFT_TEXT))
        .child(key)
}
