//! WebAssembly bindings for `symbolic-debuginfo`, published to npm as
//! `@sentry/symbolic`.
//!
//! Parses debug information files (Mach-O/dSYM, ELF, PE/PDB, Portable PDB,
//! WebAssembly, Breakpad, SourceBundle) and extracts their metadata: debug id,
//! code id, architecture, kind, and feature flags.
//!
//! The host (browser or Node) reads the file and passes the bytes in, so no
//! filesystem or `mmap` is required inside the module.
//!
//! It also enumerates the source files a debug file references and builds
//! source bundles from caller-supplied source content (the host reads the
//! files, since WebAssembly has no filesystem).

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::rc::Rc;

use serde::Serialize;
use serde_bytes::ByteBuf;
use symbolic_debuginfo::sourcebundle::SourceBundleWriter;
use symbolic_debuginfo::Archive;
use wasm_bindgen::prelude::*;

/// Per-object metadata extracted from a debug information file.
#[derive(Serialize)]
struct ObjectInfo {
    debug_id: String,
    code_id: Option<String>,
    arch: String,
    file_format: String,
    kind: String,
    has_symbols: bool,
    has_debug_info: bool,
    has_unwind_info: bool,
    has_sources: bool,
}

/// Result of parsing an archive (which may contain multiple objects, e.g. a
/// Mach-O fat binary with several arch slices).
#[derive(Serialize)]
struct ArchiveInfo {
    file_format: String,
    objects: Vec<ObjectInfo>,
}

/// Parse a debug information file from an in-memory byte buffer and return its
/// metadata as a JS object. Returns an error if the buffer cannot be parsed as
/// a known object format.
#[wasm_bindgen]
pub fn parse_debug_file(data: &[u8]) -> Result<JsValue, JsError> {
    let archive = Archive::parse(data).map_err(|e| JsError::new(&e.to_string()))?;

    let mut objects = Vec::new();
    for object in archive.objects() {
        let object = object.map_err(|e| JsError::new(&e.to_string()))?;
        objects.push(ObjectInfo {
            debug_id: object.debug_id().to_string(),
            code_id: object.code_id().map(|c| c.to_string()),
            arch: object.arch().name().to_owned(),
            file_format: object.file_format().name().to_owned(),
            kind: object.kind().name().to_owned(),
            has_symbols: object.has_symbols(),
            has_debug_info: object.has_debug_info(),
            has_unwind_info: object.has_unwind_info(),
            has_sources: object.has_sources(),
        });
    }

    let info = ArchiveInfo {
        file_format: archive.file_format().name().to_owned(),
        objects,
    };

    serde_wasm_bindgen::to_value(&info).map_err(|e| JsError::new(&e.to_string()))
}

/// Detect the object file format without a full parse. Returns the format name
/// (e.g. `elf`, `macho`, `pdb`, `breakpad`, `unknown`).
#[wasm_bindgen]
pub fn peek_format(data: &[u8]) -> String {
    Archive::peek(data).name().to_owned()
}

/// Convert any `Display` error into a `JsError`.
fn to_js<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}

/// The source files referenced by a single object in a debug file.
#[derive(Serialize)]
struct SourceListEntry {
    debug_id: String,
    arch: String,
    code_id: Option<String>,
    /// Absolute source file paths referenced by the object's debug info,
    /// excluding compiler-synthesized `<...>` entries.
    sources: Vec<String>,
}

/// Enumerate the source files referenced by each object in a debug file.
///
/// Returns one entry per object: its debug id, architecture, optional code id,
/// and the list of absolute source paths it references (excluding `<...>`
/// virtual entries). The host uses this to know which files to read from disk
/// before calling [`create_source_bundle`], or to implement `print-sources`.
#[wasm_bindgen]
pub fn list_source_files(data: &[u8]) -> Result<JsValue, JsError> {
    let archive = Archive::parse(data).map_err(to_js)?;

    let mut entries = Vec::new();
    for object in archive.objects() {
        let object = object.map_err(to_js)?;
        let session = object.debug_session().map_err(to_js)?;

        let mut sources = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for file in session.files() {
            let file = file.map_err(to_js)?;
            let path = file.abs_path_str();
            if path.starts_with('<') && path.ends_with('>') {
                continue;
            }
            if seen.insert(path.clone()) {
                sources.push(path);
            }
        }

        entries.push(SourceListEntry {
            debug_id: object.debug_id().to_string(),
            arch: object.arch().name().to_owned(),
            code_id: object.code_id().map(|c| c.to_string()),
            sources,
        });
    }

    serde_wasm_bindgen::to_value(&entries).map_err(to_js)
}

/// An in-memory `Write + Seek` sink whose buffer survives after the writer that
/// owns a clone of it is dropped (e.g. by `SourceBundleWriter::finish`).
#[derive(Clone)]
struct SharedCursor(Rc<RefCell<Cursor<Vec<u8>>>>);

impl SharedCursor {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(Cursor::new(Vec::new()))))
    }

    /// Returns a copy of the bytes written so far.
    fn bytes(&self) -> Vec<u8> {
        self.0.borrow().get_ref().clone()
    }
}

impl Write for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

impl Seek for SharedCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.0.borrow_mut().seek(pos)
    }
}

/// Build a source bundle (a `.src.zip`) for the object matching `debug_id`.
///
/// `sources` is a JS array of `[absolutePath, Uint8Array]` pairs supplying the
/// contents of the files returned by [`list_source_files`] (the host reads them
/// from disk). Files not present in `sources` are skipped.
///
/// Returns the source bundle bytes, or `undefined` if no source files were
/// bundled (so the caller can skip writing an empty bundle).
#[wasm_bindgen]
pub fn create_source_bundle(
    data: &[u8],
    debug_id: &str,
    object_name: &str,
    sources: JsValue,
) -> Result<Option<Vec<u8>>, JsError> {
    let pairs: Vec<(String, ByteBuf)> = serde_wasm_bindgen::from_value(sources).map_err(to_js)?;
    let mut contents: HashMap<String, Vec<u8>> =
        pairs.into_iter().map(|(k, v)| (k, v.into_vec())).collect();

    let archive = Archive::parse(data).map_err(to_js)?;
    let mut target = None;
    for object in archive.objects() {
        let object = object.map_err(to_js)?;
        if object.debug_id().to_string() == debug_id {
            target = Some(object);
            break;
        }
    }
    let object =
        target.ok_or_else(|| JsError::new(&format!("no object with debug id {debug_id}")))?;

    let sink = SharedCursor::new();
    let writer = SourceBundleWriter::start(sink.clone()).map_err(to_js)?;
    let written = writer
        .write_object_with_source_provider(&object, object_name, |path| contents.remove(path))
        .map_err(to_js)?;

    Ok(written.then(|| sink.bytes()))
}
