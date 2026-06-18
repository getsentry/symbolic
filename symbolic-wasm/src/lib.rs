//! WebAssembly bindings for `symbolic-debuginfo`, published to npm as
//! `@sentry/symbolic`.
//!
//! Parses debug information files (Mach-O/dSYM, ELF, PE/PDB, Portable PDB,
//! WebAssembly, Breakpad, SourceBundle) and extracts their metadata: debug id,
//! code id, architecture, kind, and feature flags.
//!
//! The host (browser or Node) reads the file and passes the bytes in, so no
//! filesystem or `mmap` is required inside the module.

use serde::Serialize;
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
