use std::io::Write;
use std::path::Path;
use std::process::Command as Process;
use std::time::Duration;

use ledger_io::*;
use ledger_model::*;

fn big_document(rows: usize) -> Document {
    let mut doc = sample::document();
    let tid = *doc.tables.keys().next().unwrap();
    for i in 0..rows {
        let row = doc.ids.row();
        let cols: Vec<ColId> = doc.tables[&tid].columns.iter().map(|c| c.id).collect();
        let cells = cols
            .iter()
            .map(|c| (*c, Cell::new(CellInput::Number(i as f64 * 1.25))))
            .collect();
        Command::InsertRow { table: tid, at: 1, row, cells }.apply(&mut doc).unwrap();
    }
    doc
}

/// Finish-line test 6: save and reopen loses nothing.
#[test]
fn save_and_reopen_loses_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.ldg");
    let doc = sample::document();
    save(&doc, &path).unwrap();
    let opened = open(&path).unwrap();
    assert_eq!(opened.doc, doc);
    assert!(!opened.read_only && !opened.recovered_from_backup);
    assert_eq!(opened.manifest.format_version, FORMAT_VERSION);
}

#[test]
fn second_save_keeps_the_first_as_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.ldg");
    let first = sample::document();
    save(&first, &path).unwrap();
    let mut second = first.clone();
    second.meta.title = "Second version".into();
    save(&second, &path).unwrap();
    assert_eq!(open(&path).unwrap().doc, second);
    assert_eq!(open_exact(&backup_path(&path)).unwrap().doc, first);
}

/// Finish-line test 7, part 1: a save stopped at any point leaves a file that opens.
#[test]
fn a_save_stopped_at_any_point_leaves_an_openable_file() {
    for fault in [Fault::DuringTempWrite, Fault::AfterTempWritten, Fault::AfterBackup] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("report.ldg");
        let old = sample::document();
        save(&old, &path).unwrap();
        let mut new = old.clone();
        new.meta.title = "Unsaved change".into();
        assert!(save_with_fault(&new, &path, Some(fault)).is_err());
        let opened = open(&path).unwrap();
        assert_eq!(opened.doc, old, "{fault:?}: the previous version is intact");
        // The next normal save still works and cleans up.
        save(&new, &path).unwrap();
        assert_eq!(open(&path).unwrap().doc, new, "{fault:?}");
    }
}

/// Finish-line test 7, part 2: a damaged file falls back to its backup.
#[test]
fn a_damaged_file_opens_from_its_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.ldg");
    let first = sample::document();
    save(&first, &path).unwrap();
    let mut second = first.clone();
    second.meta.title = "Second".into();
    save(&second, &path).unwrap();
    // Damage the main file: cut it in half.
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    let opened = open(&path).unwrap();
    assert!(opened.recovered_from_backup);
    assert_eq!(opened.doc, first);
}

/// Finish-line test 7, part 3: kill a real process while it saves, many times.
#[test]
fn killing_the_process_mid_save_never_loses_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.ldg");
    save(&big_document(10), &path).unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut completed_saves_seen = 0;
    for i in 0..12u64 {
        let mut child = Process::new(&exe)
            .args(["--exact", "child_saver", "--nocapture", "--test-threads=1"])
            .env("LEDGER_CHILD_SAVE", &path)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(150 + i * 37));
        child.kill().unwrap();
        child.wait().unwrap();
        let opened = open(&path).unwrap_or_else(|e| panic!("run {i}: file did not open: {e}"));
        assert!(!opened.doc.tables.is_empty(), "run {i}");
        if opened.doc.meta.title.starts_with("save ") {
            completed_saves_seen += 1;
        }
    }
    assert!(completed_saves_seen > 0, "the child never finished a save, so the test proved nothing");
    // A normal save cleans up temporary files left by the killed processes.
    save(&open(&path).unwrap().doc, &path).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp-") || n.contains(".bak-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// Not a real test: the body of the process that the test above kills.
#[test]
fn child_saver() {
    let Some(path) = std::env::var_os("LEDGER_CHILD_SAVE") else { return };
    let path = Path::new(&path);
    let doc = big_document(1000);
    let mut n = 0u64;
    loop {
        let mut d = doc.clone();
        d.meta.title = format!("save {n}");
        save(&d, path).unwrap();
        n += 1;
        let _ = std::io::stdout().flush();
    }
}

#[test]
fn a_file_from_a_newer_version_opens_read_only_and_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("newer.ldg");
    let doc = sample::document();
    save(&doc, &path).unwrap();
    rewrite_manifest(&path, FORMAT_VERSION + 1, MINIMUM_READER_VERSION);
    let opened = open(&path).unwrap();
    assert!(opened.read_only);
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(save(&opened.doc, &path), Err(FileError::WouldOverwriteNewer(_))));
    assert_eq!(std::fs::read(&path).unwrap(), before, "file untouched");
}

#[test]
fn a_file_that_needs_a_newer_reader_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.ldg");
    save(&sample::document(), &path).unwrap();
    rewrite_manifest(&path, FORMAT_VERSION + 5, FORMAT_VERSION + 3);
    assert!(matches!(open(&path), Err(FileError::NeedsNewerApp(v)) if v == FORMAT_VERSION + 3));
}

#[test]
fn the_zip_holds_one_json_file_per_part() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.ldg");
    let doc = sample::document();
    save(&doc, &path).unwrap();
    let zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    names.sort();
    let t = doc.tables.keys().next().unwrap().0;
    let c = doc.charts.keys().next().unwrap().0;
    let mut want = vec![
        "manifest.json".to_string(),
        "document.json".to_string(),
        format!("tables/{t}.json"),
        format!("charts/{c}.json"),
    ];
    want.sort();
    assert_eq!(names, want);
}

/// Replace manifest.json inside an existing file, keeping everything else.
fn rewrite_manifest(path: &Path, format_version: u32, minimum_reader_version: u32) {
    let bytes = std::fs::read(path).unwrap();
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().unwrap().to_string();
        out.start_file(&name, opts).unwrap();
        if name == "manifest.json" {
            let m = Manifest { format_version, minimum_reader_version, app: "future".into() };
            out.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
        } else {
            std::io::copy(&mut f, &mut out).unwrap();
        }
    }
    out.finish().unwrap();
}
