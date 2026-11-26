//! The `.ldg` file format: a zip of JSON files with a version manifest,
//! saved atomically with a backup of the previous version.
//!
//! Layout inside the zip:
//! - `manifest.json`: [`Manifest`]
//! - `document.json`: page setup, blocks, id generator
//! - `tables/<id>.json`: one [`Table`] each
//! - `charts/<id>.json`: one [`ChartSpec`] each

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ledger_model::{Block, ChartSpec, Document, IdGen, Meta, PageSetup, Table};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::write::SimpleFileOptions;

/// Format written by this build.
pub const FORMAT_VERSION: u32 = 1;
/// Oldest app format that can still read what this build writes.
pub const MINIMUM_READER_VERSION: u32 = 1;
/// Oldest format this build can migrate from.
const OLDEST_READABLE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub minimum_reader_version: u32,
    /// The app that wrote the file, for support only.
    pub app: String,
}

impl Manifest {
    fn current() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            minimum_reader_version: MINIMUM_READER_VERSION,
            app: format!("ledger {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Debug, Error)]
pub enum FileError {
    #[error("file error: {0}")]
    Io(#[from] io::Error),
    #[error("not a valid .ldg file: {0}")]
    Corrupt(String),
    #[error("this file needs a newer version of the app (format {0})")]
    NeedsNewerApp(u32),
    #[error("this file uses format {0}, which this version can no longer read")]
    TooOld(u32),
    #[error("the file on disk was written by a newer version (format {0}); save under a new name instead")]
    WouldOverwriteNewer(u32),
}

impl From<zip::result::ZipError> for FileError {
    fn from(e: zip::result::ZipError) -> Self {
        FileError::Corrupt(e.to_string())
    }
}

impl From<serde_json::Error> for FileError {
    fn from(e: serde_json::Error) -> Self {
        FileError::Corrupt(e.to_string())
    }
}

/// The result of opening a file.
#[derive(Debug)]
pub struct Opened {
    pub doc: Document,
    /// Written by a newer version: show it, but never save over it.
    pub read_only: bool,
    /// The file itself was damaged and this came from `<name>.ldg.bak`.
    pub recovered_from_backup: bool,
    pub manifest: Manifest,
}

/// `document.json`: the document without its tables and charts.
#[derive(Serialize, Deserialize)]
struct DocumentFile {
    meta: Meta,
    page: PageSetup,
    blocks: Vec<Block>,
    ids: IdGen,
}

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

fn temp_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(".{suffix}-{}", std::process::id()));
    path.with_file_name(name)
}

/// Where a save can be stopped in tests, to prove a crash at that point is safe.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Stop half way through writing the temporary file.
    DuringTempWrite,
    /// Stop after the temporary file is complete, before anything is renamed.
    AfterTempWritten,
    /// Stop after the backup is updated, before the new file replaces the old.
    AfterBackup,
}

/// Save `doc` to `path` atomically, keeping the previous file as `<path>.bak`.
pub fn save(doc: &Document, path: &Path) -> Result<(), FileError> {
    save_with_fault(doc, path, None)
}

#[doc(hidden)]
pub fn save_with_fault(doc: &Document, path: &Path, fault: Option<Fault>) -> Result<(), FileError> {
    // Never overwrite a file from a newer version. A damaged file may be replaced.
    let existing_ok = match read_manifest(path) {
        Ok(m) if m.format_version > FORMAT_VERSION => {
            return Err(FileError::WouldOverwriteNewer(m.format_version));
        }
        Ok(_) => true,
        Err(_) => false,
    };

    let bytes = encode(doc)?;
    let tmp = temp_path(path, "tmp");
    let result = (|| {
        let mut f = File::create(&tmp)?;
        if fault == Some(Fault::DuringTempWrite) {
            f.write_all(&bytes[..bytes.len() / 2])?;
            f.sync_all()?;
            return Err(stopped());
        }
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        if fault == Some(Fault::AfterTempWritten) {
            return Err(stopped());
        }
        // Keep the previous good version. Copy, then rename, so the .bak is never half written.
        if existing_ok {
            let bak_tmp = temp_path(path, "bak");
            fs::copy(path, &bak_tmp)?;
            File::open(&bak_tmp)?.sync_all()?;
            fs::rename(&bak_tmp, backup_path(path))?;
        }
        if fault == Some(Fault::AfterBackup) {
            return Err(stopped());
        }
        // Atomic replace on Linux and macOS; on Windows std uses MoveFileEx with REPLACE_EXISTING.
        fs::rename(&tmp, path)?;
        sync_dir(path);
        remove_stale_temp_files(path);
        Ok(())
    })();
    if result.is_err() && fault.is_none() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Remove temporary files left by a save that was killed part way.
fn remove_stale_temp_files(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let prefix = format!(".{}.", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if let Some(rest) = n.strip_prefix(&prefix)
            && (rest.starts_with("tmp-") || rest.starts_with("bak-"))
        {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn stopped() -> FileError {
    FileError::Io(io::Error::other("save stopped by test fault"))
}

#[cfg(unix)]
fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Ok(d) = File::open(dir)
    {
        let _ = d.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) {}

fn encode(doc: &Document) -> Result<Vec<u8>, FileError> {
    let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, value: &dyn erased::Json| -> Result<(), FileError> {
        zip.start_file(name, opts)?;
        zip.write_all(&value.to_json()?)?;
        Ok(())
    };
    put("manifest.json", &Manifest::current())?;
    put(
        "document.json",
        &DocumentFile {
            meta: doc.meta.clone(),
            page: doc.page,
            blocks: doc.blocks.clone(),
            ids: doc.ids.clone(),
        },
    )?;
    for (id, table) in &doc.tables {
        put(&format!("tables/{}.json", id.0), table)?;
    }
    for (id, chart) in &doc.charts {
        put(&format!("charts/{}.json", id.0), chart)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Lets one closure write values of different types.
mod erased {
    pub trait Json {
        fn to_json(&self) -> Result<Vec<u8>, serde_json::Error>;
    }
    impl<T: serde::Serialize> Json for T {
        fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
            serde_json::to_vec_pretty(self)
        }
    }
}

/// Open `path`. If it is damaged and a readable `.bak` exists, open that instead.
pub fn open(path: &Path) -> Result<Opened, FileError> {
    match open_exact(path) {
        Ok(o) => Ok(o),
        Err(FileError::Corrupt(why)) => {
            let bak = backup_path(path);
            match open_exact(&bak) {
                Ok(mut o) => {
                    o.recovered_from_backup = true;
                    Ok(o)
                }
                Err(_) => Err(FileError::Corrupt(why)),
            }
        }
        Err(e) => Err(e),
    }
}

/// Open exactly this file, with no fallback.
pub fn open_exact(path: &Path) -> Result<Opened, FileError> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut files: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name()?.to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        files.insert(name, serde_json::from_slice(&buf)?);
    }
    let manifest: Manifest = serde_json::from_value(
        files.remove("manifest.json").ok_or_else(|| FileError::Corrupt("no manifest.json".into()))?,
    )?;
    if manifest.minimum_reader_version > FORMAT_VERSION {
        return Err(FileError::NeedsNewerApp(manifest.minimum_reader_version));
    }
    if manifest.format_version < OLDEST_READABLE {
        return Err(FileError::TooOld(manifest.format_version));
    }
    migrate(&mut files, manifest.format_version)?;

    let doc_file: DocumentFile = serde_json::from_value(
        files.remove("document.json").ok_or_else(|| FileError::Corrupt("no document.json".into()))?,
    )?;
    let mut doc = Document {
        meta: doc_file.meta,
        page: doc_file.page,
        blocks: doc_file.blocks,
        ids: doc_file.ids,
        ..Default::default()
    };
    for (name, value) in files {
        if name.starts_with("tables/") {
            let t: Table = serde_json::from_value(value)?;
            doc.tables.insert(t.id, t);
        } else if name.starts_with("charts/") {
            let c: ChartSpec = serde_json::from_value(value)?;
            doc.charts.insert(c.id, c);
        }
    }
    Ok(Opened {
        doc,
        read_only: manifest.format_version > FORMAT_VERSION,
        recovered_from_backup: false,
        manifest,
    })
}

/// One step per format version: `MIGRATIONS[i]` upgrades format `i + 1` to `i + 2`.
/// Every step gets a fixture file in `tests/fixtures/` when it is added.
type Migration = fn(&mut BTreeMap<String, serde_json::Value>) -> Result<(), FileError>;
const MIGRATIONS: &[Migration] = &[];

fn migrate(files: &mut BTreeMap<String, serde_json::Value>, from: u32) -> Result<(), FileError> {
    let start = (from.saturating_sub(1)) as usize;
    for step in MIGRATIONS.iter().skip(start) {
        step(files)?;
    }
    Ok(())
}

/// Read only the manifest, for the newer-file check.
pub fn read_manifest(path: &Path) -> Result<Manifest, FileError> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut entry = zip.by_name("manifest.json")?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}
