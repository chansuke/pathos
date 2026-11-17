mod editor;
mod file;
mod layout;

use std::path::PathBuf;

use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathPromptOptions, PromptLevel, Task, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use pathos_core::{Document, PPQ, Project};
use pathos_playback::output::Output;
use pathos_render::Score;

use editor::{Editor, Input, Key};
use file::Saved;
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
const MUTED: u32 = 0xd19a66;

const HINT: &str = "T: track · Double-click a lane: clip · Drag: move · S: split · Delete: delete · \
                    Click the roll: note · Ctrl-Z / Ctrl-Shift-Z: undo / redo · Space: play · \
                    Home: start · Ctrl-S: save · Ctrl-O: open";

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
}

impl Pathos {
    fn handle(&mut self, input: Input, cx: &mut Context<Self>) {
        let heard = self.editor.dragged_volume();
        if self.editor.handle(input) {
            self.notice = None;
            self.publish();
        } else if self.editor.dragged_volume() != heard {
            // A dragged volume changed or was let go without an edit.
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
            output.publish(Score::new(&self.editor.heard(), output.sample_rate()));
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
        let title = self.saved.title(self.editor.document());
        if title != self.title {
            window.set_window_title(&title);
            self.title = title;
        }
        let project = self.editor.history().present();
        let selected = self.editor.selected();
        let width = f32::from(window.viewport_size().width);
        let status = match (self.editor.rejected(), &self.notice, &self.output) {
            (Some(error), _, _) => div().text_color(rgb(ERROR)).child(error.to_string()),
            (None, Some(Notice::Saved(path)), _) => {
                div().child(format!("Saved {}", path.display()))
            }
            (None, Some(Notice::Failed(error)), _) => {
                div().text_color(rgb(ERROR)).child(error.clone())
            }
            (None, None, Err(error)) => div()
                .text_color(rgb(ERROR))
                .child(format!("No audio output: {error}")),
            (None, None, Ok(_)) => div().child(HINT),
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
                if this.handle_file(event, cx) || this.handle_transport(event, cx) {
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
                if this.editor.dragged_volume().is_some() {
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
            .children(lanes(&self.editor, width))
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

/// The track headers, the lanes and a line on every bar.
fn lanes(editor: &Editor, width: f32) -> Vec<Div> {
    let project = editor.history().present();
    let mut elements = Vec::new();
    for track in 0..project.track_count() {
        let top = layout::lane_top(track);
        let muted = project.track(track).is_some_and(|track| track.muted);
        elements.push(
            at(Rect {
                x: 0.0,
                y: top + 4.0,
                width: HEADER_WIDTH,
                height: 18.0,
            })
            .px_2()
            .child(format!("Track {}", track + 1)),
        );
        let mute = at(layout::mute_rect(track))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(GRID))
            .child("M");
        elements.push(if muted {
            mute.bg(rgb(MUTED)).text_color(rgb(BACKGROUND))
        } else {
            mute
        });
        elements.push(
            at(layout::volume_rect(track))
                .flex()
                .items_center()
                .justify_end()
                .child(editor::volume_label(editor.volume(track))),
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
