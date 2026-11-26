//! Headless UI tests: the real app drawn by egui_kittest, driven through the
//! accessibility tree the way a user would click.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable, by};
use ledger_app::{FilePicker, LedgerApp, Session};
use ledger_model::{Paper, sample};

/// Answers file dialogs from a queue the test fills.
#[derive(Clone, Default)]
struct FakePicker(Rc<RefCell<VecDeque<PathBuf>>>);

impl FakePicker {
    fn answer(&self, p: PathBuf) {
        self.0.borrow_mut().push_back(p);
    }
}

impl FilePicker for FakePicker {
    fn pick_open(&mut self) -> Option<PathBuf> {
        self.0.borrow_mut().pop_front()
    }
    fn pick_save(&mut self, _suggested: &str) -> Option<PathBuf> {
        self.0.borrow_mut().pop_front()
    }
}

fn harness(session: Session) -> (Harness<'static, LedgerApp>, FakePicker) {
    let picker = FakePicker::default();
    let app = LedgerApp::new(session, Box::new(picker.clone()));
    let mut h = Harness::builder()
        .with_size([1000.0, 800.0])
        .build_ui_state(|ui, app: &mut LedgerApp| app.show(ui), app);
    h.run();
    (h, picker)
}

/// A menu item's label is its name followed by its shortcut, if it has one.
fn is_item(label: Option<String>, item: &str) -> bool {
    label.is_some_and(|l| l == item || l.starts_with(&format!("{item} Ctrl")))
}

fn has_item(h: &Harness<'_, LedgerApp>, item: &str) -> bool {
    h.query(by().predicate(move |n| is_item(n.label(), item))).is_some()
}

fn menu(h: &mut Harness<'_, LedgerApp>, top: &str, item: &str) {
    h.get_by_label(top).click();
    h.run();
    h.get(by().predicate(move |n| is_item(n.label(), item))).click();
    h.run();
}

#[test]
fn menus_have_every_p1_s2_command() {
    let (mut h, _) = harness(Session::default());
    h.get_by_label("File").click();
    h.run();
    for item in ["New", "Open…", "Save", "Save As…", "Export PDF…", "Quit"] {
        assert!(has_item(&h, item), "File menu has {item}");
    }
}

#[test]
fn page_setup_dialog_changes_the_page_and_is_undoable() {
    let (mut h, _) = harness(Session::default());
    menu(&mut h, "Format", "Page setup…");
    assert!(h.query_by_label("Page setup").is_some(), "dialog open");
    h.get_by_label("Landscape").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Page setup").is_none(), "dialog closed");
    assert!(h.state().session.doc.page.landscape);
    assert!(h.query_by_label("A4 landscape, text width 257 mm").is_some(), "status bar updated");
    menu(&mut h, "Edit", "Undo");
    assert!(!h.state().session.doc.page.landscape);
}

#[test]
fn cancel_leaves_the_page_unchanged() {
    let (mut h, _) = harness(Session::default());
    menu(&mut h, "Format", "Page setup…");
    h.get_by_label("Landscape").click();
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().session.doc.page.landscape);
    assert!(!h.state().session.is_dirty());
}

#[test]
fn paper_size_can_be_switched_to_letter() {
    let (mut h, _) = harness(Session::default());
    menu(&mut h, "Format", "Page setup…");
    h.get_by_role_and_label(eframe::egui::accesskit::Role::ComboBox, "Paper").click();
    h.run();
    h.get_by_label("Letter (8.5 × 11 in)").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(h.state().session.doc.page.paper, Paper::Letter);
}

#[test]
fn zoom_buttons_change_the_editor_width() {
    let (mut h, _) = harness(Session::default());
    let w100 = h.state().session.text_width_px();
    h.get_by_label("+").click();
    h.run();
    assert!(h.query_by_label("110%").is_some());
    assert!((h.state().session.text_width_px() - w100 * 1.1).abs() < 1e-3);
    h.get_by_label("110%").click();
    h.run();
    assert!(h.query_by_label("100%").is_some());
}

#[test]
fn save_as_then_open_through_the_menus() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("memo.ldg");
    let (mut h, picker) = harness(Session::with_document(sample::document()));
    picker.answer(path.clone());
    menu(&mut h, "File", "Save");
    assert_eq!(h.state().session.path(), Some(path.as_path()), "first Save asks for a name");
    assert!(path.exists());

    menu(&mut h, "File", "New");
    assert!(h.state().session.doc.blocks.is_empty());
    picker.answer(path.clone());
    menu(&mut h, "File", "Open…");
    assert_eq!(h.state().session.doc, sample::document());
}

#[test]
fn unsaved_changes_are_not_lost_on_new() {
    let (mut h, _) = harness(Session::default());
    menu(&mut h, "File", "Open sample document");
    menu(&mut h, "Format", "Page setup…");
    h.get_by_label("Landscape").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.state().session.is_dirty());

    menu(&mut h, "File", "New");
    assert!(h.query_by_label("Save changes?").is_some());
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().session.doc.page.landscape, "Cancel keeps the document");

    menu(&mut h, "File", "New");
    h.get_by_label("Don't save").click();
    h.run();
    assert!(h.state().session.doc.blocks.is_empty(), "Don't save discards it");
}

#[test]
fn save_from_the_unsaved_changes_dialog_saves_then_continues() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kept.ldg");
    let (mut h, picker) = harness(Session::default());
    menu(&mut h, "File", "Open sample document");
    menu(&mut h, "Format", "Page setup…");
    h.get_by_label("Landscape").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    menu(&mut h, "File", "New");
    picker.answer(path.clone());
    h.get_by_label("Save").click();
    h.run();
    assert!(h.state().session.doc.blocks.is_empty(), "went on to New");
    assert!(ledger_io::open(&path).unwrap().doc.page.landscape, "after saving the change");
}

#[test]
fn the_sample_document_shows_its_blocks() {
    let (mut h, _) = harness(Session::with_document(sample::document()));
    h.run();
    assert!(h.query_by_label("Deposits").is_some(), "heading");
    assert!(h.query_by_label_contains("Total deposits stand at").is_some(), "paragraph");
}
