mod editor;
mod file;
mod layout;
mod meter;
mod readout;

use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, FontWeight, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, PromptLevel, Rgba, Task,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, rgba, size,
};
use pathos_core::{CENTER_PAN, Clip, Document, PPQ, Project, Tick, UNITY_VOLUME};
use pathos_playback::output::Output;
use pathos_render::Score;

use editor::{Editor, Input, Key};
use file::Saved;
use layout::{
    BEAT_WIDTH, FOOTER_HEIGHT, Fader, HEADER_WIDTH, HIGHEST_PITCH, KEYBOARD_WIDTH, LANE_HEIGHT,
    LOWEST_PITCH, ROLL_BEAT_WIDTH, ROLL_RULER_HEIGHT, ROW_HEIGHT, RULER_HEIGHT, Rect, TIMELINE_TOP,
    TOP_BAR_HEIGHT, WINDOW_HEIGHT, WINDOW_WIDTH,
};
use meter::Meter;

const BACKGROUND: u32 = 0x131518;
/// The ground of the track headers.
const PANEL: u32 = 0x1a1d21;
const LANE: u32 = 0x17191d;
const BLACK_KEY_ROW: u32 = 0x16181b;
const WHITE_KEY_ROW: u32 = 0x1d2024;
const WHITE_KEY: u32 = 0xd4d8de;
/// The line between two white keys; the one below a C or an F is darker.
const WHITE_KEY_LINE: u32 = 0xc3c8cf;
const BLACK_KEY: u32 = 0x23272c;
const GRID: u32 = 0x2a2e34;
/// The line of each beat, fainter than the line of a bar.
const BEAT_LINE: u32 = 0x1e2125;
const CONTROL: u32 = 0x202328;
const CONTROL_BORDER: u32 = 0x343941;
const TEXT: u32 = 0xe6e8eb;
/// Labels and values that are read less often than names.
const SUBTLE_TEXT: u32 = 0x8a919b;
/// Text on controls that are off.
const SOFT_TEXT: u32 = 0xc9ced6;
const SELECTION: u32 = 0xf2f4f7;
const ERROR: u32 = 0xe06c75;
const PLAYHEAD: u32 = 0xf0f0f0;
const MUTED: u32 = 0xffb066;
const SOLOED: u32 = 0x67c587;
/// The colors of the tracks in turn. A track's color follows its place, so it
/// is not part of the project.
const TRACK_COLORS: [u32; 6] = [0x5aa9ff, 0xb48cff, 0x4fd1c5, 0xf48fb1, 0xe2b54a, 0x67c587];
/// The color of the play button, the loop and the toggles that are on.
const ACCENT: u32 = 0x5aa9ff;
/// The loop range over the lanes, translucent so that clips show through.
const LOOP: u32 = ACCENT << 8 | 0x12;
/// The ground of a readout, darker than the bar around it.
const READOUT: u32 = 0x0f1113;
/// The meter's zones: up to -6 dB, up to full scale, and past it.
const METER_LOW: u32 = 0x57b97a;
const METER_HIGH: u32 = 0xe2b54a;

/// The keys the footer lists, with what each does. The loop's and the
/// metronome's are on their buttons and the track's on the ruler.
const KEYS: [(&str, &str); 9] = [
    ("Space", "play"),
    ("Home", "start"),
    ("S", "split"),
    ("Delete", "delete"),
    ("Ctrl Z", "undo"),
    ("Ctrl Shift Z", "redo"),
    ("Ctrl S", "save"),
    ("Ctrl Shift S", "save as"),
    ("Ctrl O", "open"),
];

/// The outcome of the last save or open, shown until the next accepted edit.
enum Notice {
    Saved(PathBuf),
    Failed(String),
}

struct Pathos {
    editor: Editor,
    focus: FocusHandle,
    /// The audio device, or why it could not be opened. Editing works
    /// without one.
    output: Result<Output, String>,
    saved: Saved,
    notice: Option<Notice>,
    /// The window title last set, so that it is set only when it changes.
    title: String,
    /// Whether the window is asking to save before it closes.
    asking_to_close: bool,
    /// The ticks that playback repeats, if any (ADR 22).
    looping: Option<Range<Tick>>,
    /// Whether playback clicks on every beat (ADR 23).
    metronome: bool,
    meter: Meter,
    /// When the meter last took the peaks of the audio thread.
    metered: Option<Instant>,
}

impl Pathos {
    fn handle(&mut self, input: Input, cx: &mut Context<Self>) {
        let heard = self.editor.pending();
        if self.editor.handle(input) {
            self.notice = None;
            self.publish();
        } else if self.editor.pending() != heard {
            // A dragged volume or pan changed, or was let go without an edit.
            self.publish();
        }
        cx.notify();
    }

    /// Starts editing `document`, stored in `path`, from the beginning.
    fn replace(&mut self, document: Document, path: PathBuf) {
        self.saved = Saved::new(Some(path), &document);
        self.editor = Editor::new(document);
        if let Ok(output) = &self.output {
            output.transport().stop();
            output.transport().seek(0);
        }
        self.publish();
    }

    /// Saves to the document's file, or asks for one when it has none or when
    /// `choose` is set. The task tells whether the document was saved.
    fn save(&mut self, choose: bool, cx: &mut Context<Self>) -> Task<bool> {
        if let (Some(path), false) = (self.saved.path(), choose) {
            return Task::ready(self.save_to(path.to_path_buf()));
        }
        let name = format!("untitled.{}", file::EXTENSION);
        let prompt = cx.prompt_for_new_path(&self.saved.directory(), Some(&name));
        cx.spawn(async move |this, cx| {
            let answer = prompt.await;
            this.update(cx, |this, cx| {
                let saved = match answer {
                    Ok(Ok(Some(path))) => this.save_to(path),
                    Ok(Err(error)) => {
                        this.notice =
                            Some(Notice::Failed(format!("cannot ask for a file: {error}")));
                        false
                    }
                    Ok(Ok(None)) | Err(_) => false,
                };
                cx.notify();
                saved
            })
            .unwrap_or(false)
        })
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        let saved = self.saved.save(path.clone(), self.editor.document());
        self.notice = Some(match &saved {
            Ok(()) => Notice::Saved(path),
            Err(error) => Notice::Failed(format!("cannot save {}: {error}", path.display())),
        });
        saved.is_ok()
    }

    /// Whether the window may close now. With unsaved changes it asks first,
    /// and closes the window itself once they are saved or discarded.
    fn may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.saved.modified(self.editor.document()) {
            return true;
        }
        if self.asking_to_close {
            return false;
        }
        self.asking_to_close = true;
        let name = self
            .saved
            .path()
            .and_then(|path| path.file_name())
            .map_or("untitled".into(), |name| name.to_string_lossy());
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save the changes to {name}?"),
            Some("Unsaved changes will be lost."),
            &["Save", "Close Without Saving", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let close = match answer.await {
                Ok(0) => match this.update(cx, |this, cx| this.save(false, cx)) {
                    Ok(saving) => saving.await,
                    Err(_) => false,
                },
                Ok(1) => true,
                _ => false,
            };
            let _ = this.update(cx, |this, _| this.asking_to_close = false);
            if close {
                let _ = cx.update(|window, _| window.remove_window());
            }
        })
        .detach();
        false
    }

    /// Asks for a file and opens it, unless there are unsaved changes.
    fn open(&mut self, cx: &mut Context<Self>) {
        if self.saved.modified(self.editor.document()) {
            self.notice = Some(Notice::Failed(
                "there are unsaved changes; save them first".into(),
            ));
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let answer = prompt.await;
            this.update(cx, |this, cx| {
                match answer {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            this.open_path(path);
                        }
                    }
                    Ok(Err(error)) => {
                        this.notice =
                            Some(Notice::Failed(format!("cannot ask for a file: {error}")))
                    }
                    Ok(Ok(None)) | Err(_) => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_path(&mut self, path: PathBuf) {
        match file::open(&path) {
            Ok(document) => self.replace(document, path),
            Err(error) => {
                self.notice = Some(Notice::Failed(format!(
                    "cannot open {}: {error}",
                    path.display()
                )))
            }
        }
    }

    fn handle_file(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let modifiers = &event.keystroke.modifiers;
        if !modifiers.secondary() {
            return false;
        }
        match event.keystroke.key.as_str() {
            "s" => self.save(modifiers.shift, cx).detach(),
            "o" => self.open(cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Hands the project as it should sound now to the audio thread (ADR 18).
    fn publish(&self) {
        if let Ok(output) = &self.output {
            let (heard, rate) = (self.editor.heard(), output.sample_rate());
            let mut score = Score::new(&heard, rate);
            if let Some(ticks) = &self.looping {
                score = score.looped(ticks.clone());
            }
            if self.metronome {
                score = score.with_metronome();
            }
            output.publish(score);
        }
    }

    /// Loops the selected clip, or every clip, and stops looping when a loop
    /// is on (ADR 22).
    fn toggle_loop(&mut self, cx: &mut Context<Self>) {
        self.looping = match self.looping {
            Some(_) => None,
            None => editor::loop_range(self.editor.history().present(), self.editor.selected()),
        };
        self.publish();
        cx.notify();
    }

    fn toggle_metronome(&mut self, cx: &mut Context<Self>) {
        self.metronome = !self.metronome;
        self.publish();
        cx.notify();
    }

    fn play_or_stop(&mut self, cx: &mut Context<Self>) {
        if let Ok(output) = &self.output {
            let transport = output.transport();
            if transport.is_playing() {
                transport.stop();
            } else {
                transport.play();
            }
            cx.notify();
        }
    }

    fn rewind(&mut self, cx: &mut Context<Self>) {
        if let Ok(output) = &self.output {
            output.transport().seek(0);
            cx.notify();
        }
    }

    fn handle_transport(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        match event.keystroke.key.as_str() {
            "space" => self.play_or_stop(cx),
            "home" => self.rewind(cx),
            _ => return false,
        }
        true
    }

    /// The bar at the top: the file, the transport, the readout, the loop and
    /// metronome toggles, the master volume and the meter (ADR 27).
    fn top_bar(&self, width: f32, tick: Tick, frame: u64, cx: &mut Context<Self>) -> Vec<Div> {
        let project = self.editor.history().present();
        let playing = self
            .output
            .as_ref()
            .is_ok_and(|output| output.transport().is_playing());
        let name = self
            .saved
            .path()
            .and_then(|path| path.file_name())
            .map_or("untitled".into(), |name| {
                name.to_string_lossy().into_owned()
            });
        let modified = self.saved.modified(self.editor.document());
        let time = match &self.output {
            Ok(output) => readout::time_label(frame, output.sample_rate()),
            Err(_) => "-:--.---".into(),
        };
        let mut elements = vec![
            at(Rect {
                x: 0.0,
                y: 0.0,
                width,
                height: TOP_BAR_HEIGHT,
            })
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(GRID)),
            at(Rect {
                x: 16.0,
                y: 0.0,
                width: layout::home_rect().x - 32.0,
                height: TOP_BAR_HEIGHT,
            })
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("pathos"),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(rgb(CONTROL_BORDER)))
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(SOFT_TEXT))
                    .child(name),
            )
            .children(modified.then(|| {
                div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded(px(3.0))
                    .bg(rgb(MUTED))
            })),
            at(Rect {
                x: layout::home_rect().x - 4.0,
                y: layout::home_rect().y - 3.0,
                width: layout::play_rect().x + layout::play_rect().width - layout::home_rect().x
                    + 8.0,
                height: layout::home_rect().height + 6.0,
            })
            .rounded(px(8.0))
            .bg(rgb(BACKGROUND))
            .border_1()
            .border_color(rgb(GRID)),
            at(layout::home_rect())
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_color(rgb(SOFT_TEXT))
                .child("⏮")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.rewind(cx);
                    }),
                ),
            at(layout::play_rect())
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(rgb(ACCENT))
                .text_color(rgb(BACKGROUND))
                .child(if playing { "■" } else { "▶" })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.play_or_stop(cx);
                    }),
                ),
            readout_view(
                &readout::position_label(tick),
                &time,
                &readout::tempo_label(project, tick),
            ),
            toggle_view(
                layout::loop_button_rect(),
                "Loop",
                "L",
                self.looping.is_some(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.toggle_loop(cx);
                }),
            ),
            toggle_view(
                layout::metronome_button_rect(),
                "Metronome",
                "K",
                self.metronome,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.toggle_metronome(cx);
                }),
            ),
        ];
        let master = layout::master_rect(width);
        elements.push(
            at(Rect {
                y: 8.0,
                height: 14.0,
                ..master
            })
            .flex()
            .justify_end()
            .child(caption("Master")),
        );
        elements.push(
            at(master)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(GRID))
                .bg(rgb(CONTROL))
                .child(editor::volume_label(self.editor.volume(Fader::Master))),
        );
        elements.extend(meter_view(width, self.meter.levels()));
        elements
    }
}

/// A small label above a value.
fn caption(text: &'static str) -> Div {
    div()
        .text_size(px(10.0))
        .text_color(rgb(SUBTLE_TEXT))
        .child(text.to_uppercase())
}

/// A key as the footer and the toggles show it.
fn key_cap(key: &'static str) -> Div {
    div()
        .px(px(5.0))
        .rounded(px(4.0))
        .border_1()
        .border_b_2()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(CONTROL))
        .text_size(px(11.0))
        .text_color(rgb(SOFT_TEXT))
        .child(key)
}

/// The position, the time and the tempo, each under its caption.
fn readout_view(position: &str, time: &str, tempo: &str) -> Div {
    let cell = |name: &'static str, value: &str, width: f32| {
        div()
            .w(px(width))
            .h_full()
            .px(px(14.0))
            .flex()
            .flex_col()
            .justify_center()
            .child(caption(name))
            .child(div().text_size(px(16.0)).child(value.to_string()))
    };
    at(layout::readout_rect())
        .flex()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(GRID))
        .bg(rgb(READOUT))
        .child(cell("Bar", position, 92.0))
        .child(div().w(px(1.0)).h_full().bg(rgb(GRID)))
        .child(cell("Time", time, 108.0))
        .child(div().w(px(1.0)).h_full().bg(rgb(GRID)))
        .child(cell("Tempo", tempo, 82.0))
}

/// A button that turns something on and off, with its key.
fn toggle_view(rect: Rect, label: &'static str, key: &'static str, on: bool) -> Div {
    let button = at(rect)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded(px(6.0))
        .border_1()
        .text_sm()
        .child(label)
        .child(key_cap(key));
    if on {
        button
            .bg(rgba(ACCENT << 8 | 0x29))
            .border_color(rgba(ACCENT << 8 | 0xb3))
    } else {
        button
            .bg(rgb(CONTROL))
            .border_color(rgb(GRID))
            .text_color(rgb(SOFT_TEXT))
    }
}

/// The meter of each channel in its zones, a line at full scale and the
/// scale below (ADR 25).
fn meter_view(width: f32, levels: [f32; 2]) -> Vec<Div> {
    let decibels = |decibels: f32| 10f32.powf(decibels / 20.0);
    let left = layout::meter_rect(width, 0);
    let at_level = |level: f32| left.width * meter::fraction(level);
    let (high, full) = (at_level(decibels(-6.0)), at_level(1.0));
    let mut elements = Vec::new();
    for (channel, level) in levels.into_iter().enumerate() {
        let rect = layout::meter_rect(width, channel);
        let filled = at_level(level);
        let zone = |from: f32, to: f32, color: u32| {
            at(Rect {
                x: rect.x + from,
                width: (filled.min(to) - from).max(0.0),
                ..rect
            })
            .bg(rgb(color))
        };
        elements.push(at(rect).rounded(px(2.0)).bg(rgb(GRID)));
        elements.push(zone(0.0, high, METER_LOW));
        elements.push(zone(high, full, METER_HIGH));
        elements.push(zone(full, rect.width, ERROR));
        elements.push(
            at(Rect {
                x: rect.x + full,
                width: 1.0,
                ..rect
            })
            .bg(rgb(PANEL)),
        );
    }
    let below = layout::meter_rect(width, 1);
    elements.extend([-60.0, -24.0, -12.0, -6.0, 0.0, 6.0].map(|level: f32| {
        let x = below.x + at_level(decibels(level)).clamp(8.0, below.width - 8.0);
        at(Rect {
            x: x - 16.0,
            y: below.y + below.height + 2.0,
            width: 32.0,
            height: 12.0,
        })
        .flex()
        .justify_center()
        .text_size(px(9.0))
        .text_color(rgb(SUBTLE_TEXT))
        .child(if level > 0.0 {
            format!("+{level}")
        } else {
            format!("{level}")
        })
    }));
    elements
}

/// The ruler between the top bar and the lanes: the number of each bar, a
/// tick on each beat, the loop and where the playhead is.
fn ruler(
    project: &Project,
    looping: Option<&Range<Tick>>,
    playhead: Option<f32>,
    width: f32,
) -> Vec<Div> {
    let top = TOP_BAR_HEIGHT;
    let mut elements = vec![
        at(Rect {
            x: 0.0,
            y: top,
            width: HEADER_WIDTH,
            height: RULER_HEIGHT,
        })
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .bg(rgb(PANEL))
        .border_b_1()
        .border_r_1()
        .border_color(rgb(GRID))
        .child(caption("Tracks"))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_color(rgb(SUBTLE_TEXT))
                .child(key_cap("T"))
                .child("add"),
        ),
        at(Rect {
            x: HEADER_WIDTH,
            y: top,
            width: width - HEADER_WIDTH,
            height: RULER_HEIGHT,
        })
        .bg(rgb(LANE))
        .border_b_1()
        .border_color(rgb(GRID)),
    ];
    let beats = ((width - HEADER_WIDTH) / BEAT_WIDTH) as u64;
    for beat in 0..=beats {
        let x = HEADER_WIDTH + beat as f32 * BEAT_WIDTH;
        if beat % 4 == 0 {
            elements.push(
                at(Rect {
                    x,
                    y: top,
                    width: 1.0,
                    height: RULER_HEIGHT,
                })
                .bg(rgb(CONTROL_BORDER)),
            );
            elements.push(
                at(Rect {
                    x: x + 6.0,
                    y: top + 4.0,
                    width: 4.0 * BEAT_WIDTH - 8.0,
                    height: 14.0,
                })
                .text_size(px(11.0))
                .text_color(rgb(SUBTLE_TEXT))
                .child((beat / 4 + 1).to_string()),
            );
        } else {
            elements.push(
                at(Rect {
                    x,
                    y: top + RULER_HEIGHT - 5.0,
                    width: 1.0,
                    height: 5.0,
                })
                .bg(rgb(CONTROL_BORDER)),
            );
        }
    }
    if let Some(ticks) = looping {
        let lanes = layout::loop_rect(project, ticks);
        elements.push(
            at(Rect {
                y: top + 4.0,
                height: 10.0,
                ..lanes
            })
            .border_t_2()
            .border_l_2()
            .border_r_2()
            .rounded_t(px(3.0))
            .border_color(rgb(ACCENT)),
        );
    }
    if let Some(x) = playhead {
        elements.push(
            at(Rect {
                x: x - 6.0,
                y: TIMELINE_TOP - 11.0,
                width: 13.0,
                height: 11.0,
            })
            .flex()
            .justify_center()
            .text_size(px(10.0))
            .text_color(rgb(PLAYHEAD))
            .child("▼"),
        );
    }
    elements
}

/// The bar at the bottom: the status when there is one, else the keys.
fn footer(status: Option<(String, bool)>, width: f32, height: f32) -> Div {
    let bar = at(Rect {
        x: 0.0,
        y: height - FOOTER_HEIGHT,
        width,
        height: FOOTER_HEIGHT,
    })
    .flex()
    .items_center()
    .gap_4()
    .px_4()
    .bg(rgb(PANEL))
    .border_t_1()
    .border_color(rgb(GRID))
    .text_color(rgb(SUBTLE_TEXT));
    match status {
        Some((message, error)) => bar
            .text_color(rgb(if error { ERROR } else { SOFT_TEXT }))
            .child(div().truncate().child(message)),
        None => bar.children(KEYS.map(|(key, action)| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap_1()
                .child(key_cap(key))
                .child(action)
        })),
    }
}

impl Render for Pathos {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.saved.title(self.editor.document());
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
        let width = f32::from(window.viewport_size().width);
        let height = f32::from(window.viewport_size().height);
        self.editor.resize(width);
        let status = match (self.editor.rejected(), &self.notice, &self.output) {
            (Some(error), _, _) => Some((error.to_string(), true)),
            (None, Some(Notice::Saved(path)), _) => {
                Some((format!("Saved {}", path.display()), false))
            }
            (None, Some(Notice::Failed(error)), _) => Some((error.clone(), true)),
            (None, None, Err(error)) => Some((format!("No audio output: {error}"), true)),
            (None, None, Ok(_)) => None,
        };
        if let Ok(output) = &self.output {
            let now = Instant::now();
            let elapsed = self.metered.map_or(Duration::ZERO, |then| now - then);
            self.metered = Some(now);
            self.meter.update(output.transport().take_peaks(), elapsed);
            // Keep drawing until the meter has fallen back after playback.
            if !self.meter.is_empty() {
                window.request_animation_frame();
            }
        }
        let project = self.editor.history().present();
        let selected = self.editor.selected();
        let (tick, frame, playhead) = match &self.output {
            Ok(output) => {
                output.collect();
                let transport = output.transport();
                if transport.is_playing() {
                    window.request_animation_frame();
                }
                let frame = transport.position();
                let tick = layout::tick_at(project, output.sample_rate(), frame);
                let x = layout::playhead_x(project, output.sample_rate(), frame);
                (tick, frame, Some(x))
            }
            Err(_) => (0, 0, None),
        };
        let top_bar = self.top_bar(width, tick, frame, cx);
        let project = self.editor.history().present();

        div()
            .size_full()
            .relative()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_xs()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.handle_file(event, cx) || this.handle_transport(event, cx) {
                    return;
                }
                let modifiers = &event.keystroke.modifiers;
                if event.keystroke.key == "l" && !modifiers.secondary() {
                    this.toggle_loop(cx);
                    return;
                }
                if event.keystroke.key == "k" && !modifiers.secondary() {
                    this.toggle_metronome(cx);
                    return;
                }
                if let Some(key) = key(event) {
                    this.handle(Input::Key(key), cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                    this.handle(
                        Input::Down {
                            x,
                            y,
                            clicks: event.click_count,
                        },
                        cx,
                    );
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                if this.editor.pending().is_some() {
                    this.handle(Input::Move { x, y }, cx);
                } else {
                    this.editor.handle(Input::Move { x, y });
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                    this.handle(Input::Up { x, y }, cx);
                }),
            )
            .children(top_bar)
            .children(ruler(project, self.looping.as_ref(), playhead, width))
            .children(lanes(&self.editor, width))
            .children(
                project
                    .clips()
                    .iter()
                    .map(|clip| clip_view(project, clip, Some(clip.id) == selected)),
            )
            .children(
                self.looping
                    .as_ref()
                    .map(|ticks| at(layout::loop_rect(project, ticks)).bg(rgba(LOOP))),
            )
            .children(selected.map(|id| piano_roll(project, id, width)))
            .children(playhead.map(|x| {
                at(Rect {
                    x,
                    y: TIMELINE_TOP,
                    width: 1.0,
                    height: project.track_count() as f32 * LANE_HEIGHT,
                })
                .bg(rgb(PLAYHEAD))
            }))
            .child(footer(status, width, height))
    }
}

/// The color of `track`, `alpha` of 255 opaque.
fn track_color(track: u64, alpha: u8) -> Rgba {
    let color = TRACK_COLORS[(track % TRACK_COLORS.len() as u64) as usize];
    rgba(color << 8 | u32::from(alpha))
}

/// The track headers, the lanes and a line on every beat, stronger on a bar.
fn lanes(editor: &Editor, width: f32) -> Vec<Div> {
    let project = editor.history().present();
    let mut elements = Vec::new();
    for track in 0..project.track_count() {
        let top = layout::lane_top(track);
        let settings = project.track(track).unwrap_or_default();
        elements.push(
            at(Rect {
                x: 0.0,
                y: top,
                width: HEADER_WIDTH,
                height: LANE_HEIGHT,
            })
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(GRID)),
        );
        elements.push(
            at(Rect {
                x: 0.0,
                y: top,
                width: 4.0,
                height: LANE_HEIGHT,
            })
            .bg(track_color(track, if settings.muted { 0x59 } else { 0xff })),
        );
        elements.push(
            at(layout::name_rect(track))
                .flex()
                .items_center()
                .text_sm()
                .child(format!("Track {}", track + 1)),
        );
        let button = |rect, label: &'static str, on: bool, color: u32| {
            let button = at(rect)
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_1()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label);
            if on {
                button
                    .bg(rgb(color))
                    .border_color(rgb(color))
                    .text_color(rgb(BACKGROUND))
            } else {
                button.bg(rgb(CONTROL)).border_color(rgb(CONTROL_BORDER))
            }
        };
        elements.push(button(layout::mute_rect(track), "M", settings.muted, MUTED));
        elements.push(button(layout::solo_rect(track), "S", settings.solo, SOLOED));
        elements.extend(pan_view(track, editor.pan(track)));
        elements.extend(fader_view(track, editor.volume(Fader::Track(track))));
        elements.push(
            at(Rect {
                x: HEADER_WIDTH,
                y: top,
                width: width - HEADER_WIDTH,
                height: LANE_HEIGHT,
            })
            .bg(rgb(LANE))
            .border_b_1()
            .border_color(rgb(BEAT_LINE)),
        );
    }
    let beats = ((width - HEADER_WIDTH) / BEAT_WIDTH) as u64;
    elements.extend((0..=beats).map(|beat| {
        at(Rect {
            x: HEADER_WIDTH + beat as f32 * BEAT_WIDTH,
            y: TIMELINE_TOP,
            width: 1.0,
            height: project.track_count() as f32 * LANE_HEIGHT,
        })
        .bg(rgb(if beat % 4 == 0 { GRID } else { BEAT_LINE }))
    }));
    elements
}

/// The pan of `track`: a bar that grows from its center toward the side the
/// track is panned to, over its label.
fn pan_view(track: u64, pan: u8) -> [Div; 4] {
    let rect = layout::pan_rect(track);
    let bar = Rect {
        y: rect.y + 4.0,
        height: 4.0,
        ..rect
    };
    let center = bar.x + bar.width / 2.0;
    let offset = (f32::from(pan) - f32::from(CENTER_PAN)) / f32::from(CENTER_PAN) * bar.width / 2.0;
    [
        at(bar).rounded(px(2.0)).bg(rgb(GRID)),
        at(Rect {
            x: center,
            y: bar.y - 2.0,
            width: 1.0,
            height: 8.0,
        })
        .bg(rgb(SUBTLE_TEXT)),
        at(Rect {
            x: center + offset.min(0.0),
            width: offset.abs(),
            ..bar
        })
        .rounded(px(2.0))
        .bg(track_color(track, 0xcc)),
        at(Rect {
            y: bar.y + 6.0,
            height: rect.height - 10.0,
            ..rect
        })
        .flex()
        .justify_center()
        .text_size(px(10.0))
        .text_color(rgb(SUBTLE_TEXT))
        .child(editor::pan_label(pan)),
    ]
}

/// The volume of `track`: a fader on the meter's scale (ADR 25) and its level
/// in decibels.
fn fader_view(track: u64, volume: u16) -> [Div; 4] {
    const VALUE_WIDTH: f32 = 56.0;
    let rect = layout::volume_rect(track);
    let groove = Rect {
        y: rect.y + rect.height / 2.0 - 2.0,
        width: rect.width - VALUE_WIDTH - 8.0,
        height: 4.0,
        ..rect
    };
    let position = groove.width * meter::fraction(f32::from(volume) / f32::from(UNITY_VOLUME));
    [
        at(groove).rounded(px(2.0)).bg(rgb(GRID)),
        at(Rect {
            width: position,
            ..groove
        })
        .rounded(px(2.0))
        .bg(track_color(track, 0xb3)),
        at(Rect {
            x: groove.x + position - 2.0,
            y: groove.y - 4.0,
            width: 4.0,
            height: 12.0,
        })
        .rounded(px(2.0))
        .bg(rgb(TEXT)),
        at(Rect {
            x: rect.x + rect.width - VALUE_WIDTH,
            width: VALUE_WIDTH,
            ..rect
        })
        .flex()
        .items_center()
        .justify_end()
        .text_size(px(11.0))
        .child(editor::volume_label(volume)),
    ]
}

/// A clip in the color of its track, fainter when the track is muted, with a
/// strip naming its bar and its notes drawn small below it.
fn clip_view(project: &Project, clip: &Clip, selected: bool) -> Div {
    let muted = project.track(clip.track).is_some_and(|track| track.muted);
    let [fill, border, strip, notes] = if muted {
        [0x14, 0x40, 0x2e, 0x59]
    } else {
        [0x33, 0x8c, 0x80, 0xf2]
    };
    let rect = layout::clip_rect(clip);
    let view = at(Rect {
        x: rect.x + 1.0,
        y: rect.y + 5.0,
        width: (rect.width - 2.0).max(1.0),
        height: rect.height - 10.0,
    })
    .rounded(px(5.0))
    .overflow_hidden()
    .bg(track_color(clip.track, fill))
    .child(
        div()
            .h(px(CLIP_STRIP))
            .px_1()
            .text_size(px(10.0))
            .text_color(rgb(if muted { SUBTLE_TEXT } else { TEXT }))
            .bg(track_color(clip.track, strip))
            .child(format!("bar {}", clip.start / (4 * PPQ) + 1)),
    )
    .children(thumbnail(project, clip, track_color(clip.track, notes)));
    if selected {
        view.border_2().border_color(rgb(SELECTION))
    } else {
        view.border_1()
            .border_color(track_color(clip.track, border))
    }
}

/// The height of the strip at the top of a clip.
const CLIP_STRIP: f32 = 16.0;

/// The notes of a clip drawn small inside it below its strip, spread over the
/// pitches it uses.
fn thumbnail(project: &Project, clip: &Clip, color: Rgba) -> Vec<Div> {
    let notes: Vec<_> = project.notes(clip.id).collect();
    let (Some(low), Some(high)) = (
        notes.iter().map(|note| note.pitch).min(),
        notes.iter().map(|note| note.pitch).max(),
    ) else {
        return Vec::new();
    };
    let rows = f32::from(high - low + 1);
    let top = CLIP_STRIP + 4.0;
    let row = (LANE_HEIGHT - 10.0 - top - 4.0) / rows;
    let clip_width = layout::clip_rect(clip).width;
    notes
        .iter()
        .map(|note| {
            at(Rect {
                x: note.start as f32 / clip.length as f32 * clip_width,
                y: top + f32::from(high - note.pitch) * row,
                width: (note.length as f32 / clip.length as f32 * clip_width).max(1.0),
                height: row.clamp(1.0, 3.0),
            })
            .bg(color)
        })
        .collect()
}

/// The piano roll of the clip `id`: a ruler naming the clip and numbering its
/// beats, a keyboard, a row per pitch, a line per sixteenth, stronger on a
/// beat, and the notes in the color of the clip's track (ADR 28).
fn piano_roll(project: &Project, id: u64, width: f32) -> Div {
    let Some(clip) = project.clips().iter().find(|clip| clip.id == id) else {
        return div();
    };
    let top = layout::roll_top(project);
    let ruler = top - ROLL_RULER_HEIGHT;
    let roll_width = layout::roll_width(clip);
    let bar = clip.start / (4 * PPQ) + 1;
    let keyboard = HEADER_WIDTH - KEYBOARD_WIDTH;
    let mut elements = vec![
        at(Rect {
            x: 0.0,
            y: ruler,
            width,
            height: ROLL_RULER_HEIGHT + layout::roll_height(),
        })
        .bg(rgb(PANEL))
        .border_t_1()
        .border_color(rgb(GRID)),
        at(Rect {
            x: 12.0,
            y: ruler,
            width: keyboard - 12.0,
            height: ROLL_RULER_HEIGHT,
        })
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_none()
                .size(px(8.0))
                .rounded(px(2.0))
                .bg(track_color(clip.track, 0xff)),
        )
        .child(
            div()
                .truncate()
                .child(format!("Track {} / bar {bar}", clip.track + 1)),
        ),
    ];
    for beat in 0..clip.length.div_ceil(PPQ) {
        let x = HEADER_WIDTH + beat as f32 * ROLL_BEAT_WIDTH;
        elements.push(
            at(Rect {
                x,
                y: ruler,
                width: ROLL_BEAT_WIDTH,
                height: ROLL_RULER_HEIGHT,
            })
            .flex()
            .items_center()
            .pl(px(6.0))
            .border_l_1()
            .border_color(rgb(CONTROL_BORDER))
            .text_size(px(10.0))
            .text_color(rgb(SUBTLE_TEXT))
            .child(readout::beat_label(clip.start + beat * PPQ)),
        );
    }
    for pitch in LOWEST_PITCH..=HIGHEST_PITCH {
        let y = layout::row_top(project, pitch);
        let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
        let row = at(Rect {
            x: HEADER_WIDTH,
            y,
            width: roll_width,
            height: ROW_HEIGHT,
        })
        .bg(rgb(if black { BLACK_KEY_ROW } else { WHITE_KEY_ROW }));
        elements.push(if pitch % 12 == 0 {
            row.border_b_1().border_color(rgb(GRID))
        } else {
            row
        });
        let key = Rect {
            x: keyboard,
            y,
            width: if black {
                KEYBOARD_WIDTH * 0.6
            } else {
                KEYBOARD_WIDTH
            },
            height: ROW_HEIGHT,
        };
        elements.push(if black {
            at(key).rounded_r(px(2.0)).bg(rgb(BLACK_KEY))
        } else {
            at(key)
                .flex()
                .items_center()
                .justify_end()
                .pr(px(4.0))
                .bg(rgb(WHITE_KEY))
                .border_b_1()
                .border_color(rgb(if matches!(pitch % 12, 0 | 5) {
                    SUBTLE_TEXT
                } else {
                    WHITE_KEY_LINE
                }))
                .text_size(px(7.0))
                .text_color(rgb(BLACK_KEY))
                .children((pitch % 12 == 0).then(|| format!("C{}", i32::from(pitch / 12) - 1)))
        });
    }
    for sixteenth in 0..=clip.length / (PPQ / 4) {
        elements.push(
            at(Rect {
                x: HEADER_WIDTH + sixteenth as f32 * ROLL_BEAT_WIDTH / 4.0,
                y: top,
                width: 1.0,
                height: layout::roll_height(),
            })
            .bg(rgb(if sixteenth % 4 == 0 {
                CONTROL_BORDER
            } else {
                GRID
            })),
        );
    }
    elements.extend(
        project
            .notes(id)
            .filter_map(|note| layout::note_rect(project, note))
            .map(|rect| {
                at(rect)
                    .rounded(px(2.0))
                    .bg(track_color(clip.track, 0xff))
                    .border_1()
                    .border_color(rgba(0x0000_0059))
            }),
    );
    div().children(elements)
}

/// An element placed at `rect` of its parent.
fn at(rect: Rect) -> Div {
    div()
        .absolute()
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
}

fn key(event: &KeyDownEvent) -> Option<Key> {
    let modifiers = &event.keystroke.modifiers;
    match event.keystroke.key.as_str() {
        "z" if modifiers.secondary() && modifiers.shift => Some(Key::Redo),
        "z" if modifiers.secondary() => Some(Key::Undo),
        "y" if modifiers.secondary() => Some(Key::Redo),
        _ if modifiers.secondary() => None,
        "t" => Some(Key::AddTrack),
        "s" => Some(Key::SplitClip),
        "delete" | "backspace" => Some(Key::DeleteClip),
        _ => None,
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("pathos".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window);
                    let this = cx.entity().downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        this.update(cx, |this: &mut Pathos, cx| this.may_close(window, cx))
                            .unwrap_or(true)
                    });
                    let output = Output::open().map_err(|error| error.to_string());
                    if let Err(error) = &output {
                        eprintln!("no audio output: {error}");
                    }
                    let mut pathos = Pathos {
                        editor: Editor::default(),
                        focus,
                        output,
                        saved: Saved::default(),
                        notice: None,
                        title: String::new(),
                        asking_to_close: false,
                        looping: None,
                        metronome: false,
                        meter: Meter::default(),
                        metered: None,
                    };
                    // A file named on the command line is opened, or created
                    // by the first save if it does not exist yet.
                    if let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) {
                        if path.exists() {
                            pathos.open_path(path);
                        } else {
                            pathos.saved = Saved::new(Some(path), pathos.editor.document());
                        }
                    }
                    pathos
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
