mod editor;
mod layout;

use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};
use pathos_core::{PPQ, Project};
use pathos_playback::output::Output;
use pathos_render::Score;

use editor::{Editor, Input, Key};
use layout::{
    BEAT_WIDTH, HEADER_WIDTH, HIGHEST_PITCH, LANE_HEIGHT, LOWEST_PITCH, ROLL_BEAT_WIDTH,
    ROW_HEIGHT, Rect, STATUS_HEIGHT,
};

const BACKGROUND: u32 = 0x1e1e1e;
const LANE: u32 = 0x262626;
const BLACK_KEY_ROW: u32 = 0x1a1a1a;
const GRID: u32 = 0x333333;
const CLIP: u32 = 0x3a6ea5;
const SELECTED_CLIP: u32 = 0x5b9bd5;
const NOTE: u32 = 0xe8c547;
const TEXT: u32 = 0xd0d0d0;
const ERROR: u32 = 0xe06c75;
const PLAYHEAD: u32 = 0xf0f0f0;

const HINT: &str = "Double-click a lane: add a clip · Drag a clip: move · S: split at the pointer · \
                    Delete: delete · T: add a track · Click the piano roll: add or remove a note · \
                    Ctrl-Z / Ctrl-Shift-Z: undo / redo · Space: play / stop · Home: to the start";

struct Pathos {
    editor: Editor,
    focus: FocusHandle,
    /// The audio device, or why it could not be opened. Editing works
    /// without one.
    output: Result<Output, String>,
}

impl Pathos {
    fn handle(&mut self, input: Input, cx: &mut Context<Self>) {
        if self.editor.handle(input) {
            self.publish();
        }
        cx.notify();
    }

    /// Hands the project as it is now to the audio thread (ADR 18).
    fn publish(&self) {
        if let Ok(output) = &self.output {
            output.publish(Score::new(
                self.editor.history().present(),
                output.sample_rate(),
            ));
        }
    }

    fn handle_transport(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Ok(output) = &self.output else {
            return false;
        };
        let transport = output.transport();
        match event.keystroke.key.as_str() {
            "space" if transport.is_playing() => transport.stop(),
            "space" => transport.play(),
            "home" => transport.seek(0),
            _ => return false,
        }
        cx.notify();
        true
    }
}

impl Render for Pathos {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project = self.editor.history().present();
        let selected = self.editor.selected();
        let width = f32::from(window.viewport_size().width);
        let status = match (self.editor.rejected(), &self.output) {
            (Some(error), _) => div().text_color(rgb(ERROR)).child(error.to_string()),
            (None, Err(error)) => div()
                .text_color(rgb(ERROR))
                .child(format!("No audio output: {error}")),
            (None, Ok(_)) => div().child(HINT),
        };
        let playhead = self.output.as_ref().ok().map(|output| {
            output.collect();
            let transport = output.transport();
            if transport.is_playing() {
                window.request_animation_frame();
            }
            at(Rect {
                x: layout::playhead_x(project, output.sample_rate(), transport.position()),
                y: STATUS_HEIGHT,
                width: 1.0,
                height: project.track_count() as f32 * LANE_HEIGHT,
            })
            .bg(rgb(PLAYHEAD))
        });

        div()
            .size_full()
            .relative()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .text_xs()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.handle_transport(event, cx) {
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
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, _| {
                let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                this.editor.handle(Input::Move { x, y });
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, _, cx| {
                    let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                    this.handle(Input::Up { x, y }, cx);
                }),
            )
            .child(
                at(Rect {
                    x: 8.0,
                    y: 0.0,
                    width: width - 16.0,
                    height: STATUS_HEIGHT,
                })
                .flex()
                .items_center()
                .child(status),
            )
            .children(lanes(project, width))
            .children(project.clips().iter().map(|clip| {
                let color = if Some(clip.id) == selected {
                    SELECTED_CLIP
                } else {
                    CLIP
                };
                at(layout::clip_rect(clip))
                    .bg(rgb(color))
                    .border_1()
                    .border_color(rgb(BACKGROUND))
                    .children(thumbnail(project, clip.id, clip.length))
            }))
            .children(selected.map(|id| piano_roll(project, id)))
            .children(playhead)
    }
}

/// The track names, the lanes and a line on every bar.
fn lanes(project: &Project, width: f32) -> Vec<Div> {
    let mut elements = Vec::new();
    for track in 0..project.track_count() {
        let top = layout::lane_top(track);
        elements.push(
            at(Rect {
                x: 0.0,
                y: top,
                width: HEADER_WIDTH,
                height: LANE_HEIGHT,
            })
            .px_2()
            .flex()
            .items_center()
            .child(format!("Track {}", track + 1)),
        );
        elements.push(
            at(Rect {
                x: HEADER_WIDTH,
                y: top + 1.0,
                width: width - HEADER_WIDTH,
                height: LANE_HEIGHT - 2.0,
            })
            .bg(rgb(LANE)),
        );
    }
    let bars = ((width - HEADER_WIDTH) / (4.0 * BEAT_WIDTH)) as u64;
    elements.extend((0..=bars).map(|bar| {
        at(Rect {
            x: HEADER_WIDTH + bar as f32 * 4.0 * BEAT_WIDTH,
            y: STATUS_HEIGHT,
            width: 1.0,
            height: project.track_count() as f32 * LANE_HEIGHT,
        })
        .bg(rgb(GRID))
    }));
    elements
}

/// The notes of a clip drawn small inside it, spread over the pitches it uses.
fn thumbnail(project: &Project, id: u64, length: u64) -> Vec<Div> {
    let notes: Vec<_> = project.notes(id).collect();
    let (Some(low), Some(high)) = (
        notes.iter().map(|note| note.pitch).min(),
        notes.iter().map(|note| note.pitch).max(),
    ) else {
        return Vec::new();
    };
    let rows = f32::from(high - low + 1);
    let row = (LANE_HEIGHT - 8.0) / rows;
    let beat = length as f32 / PPQ as f32;
    notes
        .iter()
        .map(|note| {
            let clip_width = beat * BEAT_WIDTH;
            at(Rect {
                x: note.start as f32 / length as f32 * clip_width,
                y: 4.0 + f32::from(high - note.pitch) * row,
                width: (note.length as f32 / length as f32 * clip_width).max(1.0),
                height: row.clamp(1.0, 3.0),
            })
            .bg(rgb(NOTE))
        })
        .collect()
}

/// The piano roll of the clip `id`: a row per pitch, a line per beat and the
/// notes of the clip.
fn piano_roll(project: &Project, id: u64) -> Div {
    let Some(clip) = project.clips().iter().find(|clip| clip.id == id) else {
        return div();
    };
    let top = layout::roll_top(project);
    let width = layout::roll_width(clip);
    let rows = (LOWEST_PITCH..=HIGHEST_PITCH).map(|pitch| {
        let y = layout::row_top(project, pitch);
        let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
        let label = (pitch % 12 == 0).then(|| format!("C{}", i32::from(pitch / 12) - 1));
        [
            at(Rect {
                x: 0.0,
                y: y - 3.0,
                width: HEADER_WIDTH - 8.0,
                height: ROW_HEIGHT + 6.0,
            })
            .flex()
            .justify_end()
            .children(label),
            at(Rect {
                x: HEADER_WIDTH,
                y,
                width,
                height: ROW_HEIGHT,
            })
            .bg(rgb(if black { BLACK_KEY_ROW } else { LANE })),
        ]
    });
    let beats = (0..=clip.length / PPQ).map(|beat| {
        at(Rect {
            x: HEADER_WIDTH + beat as f32 * ROLL_BEAT_WIDTH,
            y: top,
            width: 1.0,
            height: layout::roll_height(),
        })
        .bg(rgb(GRID))
    });
    let notes = project
        .notes(id)
        .filter_map(|note| layout::note_rect(project, note))
        .map(|rect| {
            at(rect)
                .bg(rgb(NOTE))
                .border_1()
                .border_color(rgb(BACKGROUND))
        });
    div()
        .children(rows.flatten())
        .children(beats)
        .children(notes)
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
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
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
                    let output = Output::open().map_err(|error| error.to_string());
                    if let Err(error) = &output {
                        eprintln!("no audio output: {error}");
                    }
                    Pathos {
                        editor: Editor::default(),
                        focus,
                        output,
                    }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
