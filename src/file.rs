//! Project files on disk (ADR 19).

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use pathos_core::{Document, LoadError};

/// The extension the save prompt suggests.
pub const EXTENSION: &str = "pathos";

#[derive(Debug)]
pub enum OpenError {
    Read(io::Error),
    Load(LoadError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Read(error) => error.fmt(f),
            OpenError::Load(error) => error.fmt(f),
        }
    }
}

pub fn open(path: &Path) -> Result<Document, OpenError> {
    let file = fs::read_to_string(path).map_err(OpenError::Read)?;
    Document::load(&file).map_err(OpenError::Load)
}

/// Writes `document` to a temporary file next to `path` and renames it over
/// `path`, so that a failed save leaves the old file as it was.
pub fn save(path: &Path, document: &Document) -> io::Result<()> {
    let temporary = temporary_path(path);
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(document.save().as_bytes())?;
        file.sync_all()
    });
    match written.and_then(|()| fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// Where the open document is stored, and how many edits it had when it was
/// last saved or opened. Edits only grow, so another count means unsaved
/// changes, even after undo.
#[derive(Debug, Default)]
pub struct Saved {
    path: Option<PathBuf>,
    edits: usize,
}

impl Saved {
    /// `document` as it is in `path`, or as a new document to be saved there.
    pub fn new(path: Option<PathBuf>, document: &Document) -> Self {
        Saved {
            path,
            edits: document.edits().len(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn modified(&self, document: &Document) -> bool {
        document.edits().len() != self.edits
    }

    /// Saves `document` to `path`, which becomes its file. A failed save
    /// changes nothing.
    pub fn save(&mut self, path: PathBuf, document: &Document) -> io::Result<()> {
        save(&path, document)?;
        *self = Saved::new(Some(path), document);
        Ok(())
    }

    /// The window title: the file name, marked when there are unsaved changes.
    pub fn title(&self, document: &Document) -> String {
        let name = self
            .path()
            .and_then(Path::file_name)
            .map_or("untitled".into(), |name| name.to_string_lossy());
        let mark = if self.modified(document) { " *" } else { "" };
        format!("{name}{mark} · pathos")
    }

    /// Where the file prompts start: the folder of the document's file, or
    /// the working directory.
    pub fn directory(&self) -> PathBuf {
        self.path()
            .and_then(Path::parent)
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use pathos_core::{Command, Edit};

    use super::*;

    /// A directory of its own for each test, removed when it is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let directory =
                std::env::temp_dir().join(format!("pathos-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            Scratch(directory)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn document() -> Document {
        let mut document = Document::default();
        document.edit(Edit::Apply(Command::AddTrack)).unwrap();
        document
    }

    #[test]
    fn a_saved_file_opens_as_the_same_document_and_leaves_nothing_else() {
        let scratch = Scratch::new("round-trip");
        let path = scratch.0.join("song.pathos");
        save(&path, &document()).unwrap();
        assert_eq!(open(&path).unwrap(), document());
        let names: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["song.pathos"]);
    }

    #[test]
    fn saving_replaces_the_old_file() {
        let scratch = Scratch::new("replace");
        let path = scratch.0.join("song.pathos");
        fs::write(&path, "add-track\nadd-track\n").unwrap();
        save(&path, &document()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "add-track\n");
    }

    #[test]
    fn a_failed_save_leaves_no_temporary_file() {
        let scratch = Scratch::new("failed");
        let path = scratch.0.join("song.pathos");
        fs::create_dir(&path).unwrap();
        assert!(save(&path, &document()).is_err());
        assert!(!temporary_path(&path).exists());
    }

    #[test]
    fn edits_since_the_last_save_mark_the_document_modified() {
        let scratch = Scratch::new("modified");
        let path = scratch.0.join("song.pathos");
        let mut document = Document::default();
        let mut saved = Saved::new(Some(path.clone()), &document);
        assert!(!saved.modified(&document));
        assert_eq!(saved.title(&document), "song.pathos · pathos");
        document.edit(Edit::Apply(Command::AddTrack)).unwrap();
        assert!(saved.modified(&document));
        assert_eq!(saved.title(&document), "song.pathos * · pathos");
        saved.save(path.clone(), &document).unwrap();
        assert!(!saved.modified(&document));
        document.edit(Edit::Undo).unwrap();
        assert!(saved.modified(&document), "undo after a save is a change");
    }

    #[test]
    fn saving_under_a_new_name_moves_the_document_there() {
        let scratch = Scratch::new("save-as");
        let mut saved = Saved::default();
        assert_eq!(saved.title(&document()), "untitled * · pathos");
        assert_eq!(saved.directory(), std::env::current_dir().unwrap());
        let path = scratch.0.join("song.pathos");
        saved.save(path.clone(), &document()).unwrap();
        assert_eq!(saved.path(), Some(path.as_path()));
        assert_eq!(saved.directory(), scratch.0);
        assert_eq!(open(&path).unwrap(), document());
    }

    #[test]
    fn a_failed_save_keeps_the_document_unsaved() {
        let scratch = Scratch::new("save-fails");
        let mut saved = Saved::default();
        let directory = scratch.0.join("song.pathos");
        fs::create_dir(&directory).unwrap();
        assert!(saved.save(directory, &document()).is_err());
        assert_eq!(saved.path(), None);
        assert!(saved.modified(&document()));
    }

    #[test]
    fn opening_reports_why_a_file_cannot_be_read_or_loaded() {
        let scratch = Scratch::new("open");
        let missing = scratch.0.join("missing.pathos");
        assert!(matches!(open(&missing), Err(OpenError::Read(_))));
        let broken = scratch.0.join("broken.pathos");
        fs::write(&broken, "add-track\nplay\n").unwrap();
        let error = open(&broken).unwrap_err();
        assert_eq!(error.to_string(), "line 2: not an edit");
    }

    #[test]
    fn a_bare_relative_filename_falls_back_to_the_working_directory() {
        let bare = Saved::new(Some(PathBuf::from("song.pathos")), &Document::default());
        assert_eq!(bare.directory(), std::env::current_dir().unwrap());

        let nested = Saved::new(
            Some(PathBuf::from("projects/song.pathos")),
            &Document::default(),
        );
        assert_eq!(nested.directory(), PathBuf::from("projects"));
    }
}
