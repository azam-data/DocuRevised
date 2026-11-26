use ledger_app::session::{NeedsName, Notice, PX_PER_MM, Session};
use ledger_model::*;

fn edit(s: &mut Session, title: &str) {
    s.apply(Command::SetTitle { title: title.into() }).unwrap();
}

#[test]
fn a_new_session_is_clean_and_untitled() {
    let s = Session::default();
    assert!(!s.is_dirty() && !s.can_undo() && s.path().is_none());
    assert_eq!(s.window_title(), "Untitled - Ledger");
}

#[test]
fn edits_mark_dirty_and_undo_back_to_saved_is_clean() {
    let mut s = Session::default();
    edit(&mut s, "One");
    assert!(s.is_dirty());
    assert_eq!(s.window_title(), "*Untitled - Ledger");
    s.undo();
    assert!(!s.is_dirty(), "back to the saved state");
    s.redo();
    assert!(s.is_dirty());
}

#[test]
fn save_needs_a_name_first_then_saves_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Session::with_document(sample::document());
    assert_eq!(s.save().unwrap_err(), NeedsName::NeverSaved);
    s.save_as(&dir.path().join("report")).unwrap();
    let path = dir.path().join("report.ldg");
    assert_eq!(s.path(), Some(path.as_path()), ".ldg is added");
    assert!(!s.is_dirty());
    edit(&mut s, "Changed");
    s.save().unwrap().unwrap();
    assert!(!s.is_dirty());
    let mut other = Session::default();
    other.open(&path).unwrap();
    assert_eq!(other.doc.meta.title, "Changed");
    assert_eq!(other.window_title(), "report.ldg - Ledger");
}

#[test]
fn a_newer_file_opens_read_only_and_save_asks_for_a_new_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("newer.ldg");
    ledger_io::save(&sample::document(), &path).unwrap();
    bump_format_version(&path);
    let mut s = Session::default();
    s.open(&path).unwrap();
    assert!(s.is_read_only());
    assert!(matches!(s.notice, Some(Notice::Info(_))));
    assert!(s.window_title().contains("(read-only)"));
    assert_eq!(s.save().unwrap_err(), NeedsName::ReadOnly);
    let copy = dir.path().join("copy.ldg");
    s.save_as(&copy).unwrap();
    assert!(!s.is_read_only());
    assert_eq!(s.path(), Some(copy.as_path()));
}

#[test]
fn a_damaged_file_opens_from_backup_and_is_marked_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.ldg");
    let doc = sample::document();
    ledger_io::save(&doc, &path).unwrap();
    ledger_io::save(&doc, &path).unwrap();
    std::fs::write(&path, b"not a zip").unwrap();
    let mut s = Session::default();
    s.open(&path).unwrap();
    assert_eq!(s.doc, doc);
    assert!(s.is_dirty(), "saving will repair the damaged file");
    assert!(matches!(s.notice, Some(Notice::Error(_))));
}

#[test]
fn page_setup_is_undoable_and_invalid_setups_are_refused() {
    let mut s = Session::default();
    let mut page = s.doc.page;
    page.paper = Paper::Letter;
    page.landscape = true;
    assert!(s.set_page(page));
    assert_eq!(s.doc.page, page);
    assert!(s.is_dirty());
    s.undo();
    assert_eq!(s.doc.page, PageSetup::default());

    let mut bad = s.doc.page;
    bad.margins.left_mm = 90.0;
    bad.margins.right_mm = 90.0;
    assert!(!s.set_page(bad));
    assert_eq!(s.doc.page, PageSetup::default());
    assert!(matches!(s.notice, Some(Notice::Error(_))));
}

#[test]
fn editor_column_is_the_text_width_at_100_percent() {
    let mut s = Session::default();
    // A4 with 20 mm margins: 170 mm of text, about 642.5 screen pixels at 96 dpi.
    assert!((s.text_width_px() - 170.0 * PX_PER_MM).abs() < 1e-3);
    assert!((s.text_width_px() - 642.52).abs() < 0.01);
    s.zoom_in();
    assert_eq!(s.zoom(), 1.1);
    assert!((s.text_width_px() - 170.0 * PX_PER_MM * 1.1).abs() < 1e-3);
    s.zoom_reset();
    for _ in 0..20 {
        s.zoom_out();
    }
    assert_eq!(s.zoom(), 0.5, "stops at the smallest step");
    for _ in 0..20 {
        s.zoom_in();
    }
    assert_eq!(s.zoom(), 3.0, "stops at the largest step");
}

#[test]
fn opening_keeps_the_zoom() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("z.ldg");
    ledger_io::save(&sample::document(), &path).unwrap();
    let mut s = Session::default();
    s.zoom_in();
    s.open(&path).unwrap();
    assert_eq!(s.zoom(), 1.1);
    s.new_document();
    assert_eq!(s.zoom(), 1.1);
}

/// Rewrite the manifest to claim a newer format.
fn bump_format_version(path: &std::path::Path) {
    use std::io::Write;
    let bytes = std::fs::read(path).unwrap();
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        out.start_file(&name, zip::write::SimpleFileOptions::default()).unwrap();
        if name == "manifest.json" {
            let m = ledger_io::Manifest {
                format_version: ledger_io::FORMAT_VERSION + 1,
                minimum_reader_version: 1,
                app: "future".into(),
            };
            out.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
        } else {
            std::io::copy(&mut f, &mut out).unwrap();
        }
    }
    out.finish().unwrap();
}
