//! The open document and everything about it that is not drawing: file path,
//! undo history, unsaved changes, zoom. Kept free of egui so it can be tested
//! without a window.

use std::path::{Path, PathBuf};

use ledger_io::FileError;
use ledger_model::{Command, CommandError, Document, History, PageSetup};

/// Screen pixels per millimetre at 100% zoom (96 pixels per inch, as on the web).
pub const PX_PER_MM: f32 = 96.0 / 25.4;
/// Screen pixels per typographic point at 100% zoom.
pub const PX_PER_PT: f32 = 96.0 / 72.0;
/// The zoom levels that Zoom in and Zoom out step through.
pub const ZOOM_STEPS: [f32; 12] = [0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

pub const UNTITLED: &str = "Untitled";

/// A message for the status bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Info(String),
    Error(String),
}

/// Why a plain Save could not go ahead without asking for a file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsName {
    /// The document has never been saved.
    NeverSaved,
    /// The file was written by a newer version, so it must not be saved over.
    ReadOnly,
}

pub struct Session {
    pub doc: Document,
    history: History,
    path: Option<PathBuf>,
    /// The document as last opened or saved, to tell whether there are unsaved changes.
    saved: Document,
    dirty: bool,
    read_only: bool,
    zoom: f32,
    pub notice: Option<Notice>,
}

impl Default for Session {
    fn default() -> Self {
        Self::with_document(Document::new(UNTITLED))
    }
}

impl Session {
    /// A session for a document that has never been saved.
    pub fn with_document(doc: Document) -> Self {
        Self {
            saved: doc.clone(),
            doc,
            history: History::default(),
            path: None,
            dirty: false,
            read_only: false,
            zoom: 1.0,
            notice: None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// File name shown in the title bar, with `*` when there are unsaved changes.
    pub fn window_title(&self) -> String {
        let name = match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => UNTITLED.to_string(),
        };
        let star = if self.dirty { "*" } else { "" };
        let ro = if self.read_only { " (read-only)" } else { "" };
        format!("{star}{name}{ro} - Ledger")
    }

    // ---- Editing -------------------------------------------------------

    pub fn apply(&mut self, cmd: Command) -> Result<(), CommandError> {
        self.history.apply(&mut self.doc, cmd)?;
        self.update_dirty();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let done = self.history.undo(&mut self.doc);
        self.update_dirty();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.history.redo(&mut self.doc);
        self.update_dirty();
        done
    }

    /// Change the page setup as one undoable step. Invalid setups are refused.
    pub fn set_page(&mut self, page: PageSetup) -> bool {
        if !page.is_valid() {
            self.notice = Some(Notice::Error(format!(
                "Margins leave too little room: the text area must be at least {} mm each way.",
                PageSetup::MIN_TEXT_MM
            )));
            return false;
        }
        if page == self.doc.page {
            return true;
        }
        self.apply(Command::SetPage { page }).is_ok()
    }

    fn update_dirty(&mut self) {
        // The id generator only moves forward, so it is not part of "changed".
        let (a, b) = (&self.doc, &self.saved);
        self.dirty = a.meta != b.meta
            || a.page != b.page
            || a.blocks != b.blocks
            || a.tables != b.tables
            || a.charts != b.charts;
    }

    // ---- Files ---------------------------------------------------------

    /// Replace the session with a new, empty document.
    pub fn new_document(&mut self) {
        *self = Self { zoom: self.zoom, ..Self::default() };
    }

    /// Replace the session with `doc`, unsaved (used for the sample document).
    pub fn replace_with(&mut self, doc: Document) {
        *self = Self { zoom: self.zoom, ..Self::with_document(doc) };
    }

    pub fn open(&mut self, path: &Path) -> Result<(), FileError> {
        let opened = ledger_io::open(path)?;
        let notice = if opened.recovered_from_backup {
            Some(Notice::Error(
                "The file was damaged, so the previous version was opened from its backup. Save to repair it."
                    .into(),
            ))
        } else if opened.read_only {
            Some(Notice::Info(
                "This file was made by a newer version of Ledger. It is open read-only; use Save As to keep changes."
                    .into(),
            ))
        } else {
            None
        };
        *self = Self {
            saved: opened.doc.clone(),
            doc: opened.doc,
            history: History::default(),
            path: Some(path.to_path_buf()),
            dirty: false,
            read_only: opened.read_only,
            zoom: self.zoom,
            notice,
        };
        // A document recovered from backup differs from what is on disk.
        self.dirty = opened.recovered_from_backup;
        Ok(())
    }

    /// Save to the current path. Returns `Err(NeedsName)` when Save As is needed instead.
    pub fn save(&mut self) -> Result<Result<(), FileError>, NeedsName> {
        if self.read_only {
            return Err(NeedsName::ReadOnly);
        }
        let Some(path) = self.path.clone() else { return Err(NeedsName::NeverSaved) };
        Ok(self.write(&path))
    }

    /// Save to `path` (adding `.ldg` if it has no extension) and make it the current file.
    pub fn save_as(&mut self, path: &Path) -> Result<(), FileError> {
        let path = with_ldg_extension(path);
        self.write(&path)?;
        self.path = Some(path);
        self.read_only = false;
        Ok(())
    }

    fn write(&mut self, path: &Path) -> Result<(), FileError> {
        match ledger_io::save(&self.doc, path) {
            Ok(()) => {
                self.saved = self.doc.clone();
                self.dirty = false;
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                self.notice = Some(Notice::Info(format!("Saved {name}")));
                Ok(())
            }
            Err(e) => {
                self.notice = Some(Notice::Error(format!("Could not save: {e}")));
                Err(e)
            }
        }
    }

    // ---- Zoom ----------------------------------------------------------

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn zoom_in(&mut self) {
        if let Some(z) = ZOOM_STEPS.iter().find(|z| **z > self.zoom + 1e-3) {
            self.zoom = *z;
        }
    }

    pub fn zoom_out(&mut self) {
        if let Some(z) = ZOOM_STEPS.iter().rev().find(|z| **z < self.zoom - 1e-3) {
            self.zoom = *z;
        }
    }

    pub fn zoom_reset(&mut self) {
        self.zoom = 1.0;
    }

    /// Width of the editor column in screen pixels: the page's text width at the current zoom.
    pub fn text_width_px(&self) -> f32 {
        self.doc.page.text_width_mm() * PX_PER_MM * self.zoom
    }
}

pub fn with_ldg_extension(path: &Path) -> PathBuf {
    if path.extension().is_some() { path.to_path_buf() } else { path.with_extension("ldg") }
}
