//! WebAssembly bindings for `symbolic-debuginfo`, published to npm as
//! `@sentry/symbolic`.
//!
//! Exposes a general-purpose object API mirroring the Python bindings: an
//! [`Archive`] parses a debug information file (Mach-O/dSYM, ELF, PE/PDB,
//! Portable PDB, WebAssembly, Breakpad, SourceBundle) and yields one or more
//! [`Object`]s, each exposing its metadata (debug id, code id, architecture,
//! kind, feature flags), the source files it references, and source-bundle
//! creation.
//!
//! Everything operates on an in-memory byte buffer supplied by the host
//! (browser or Node), so no filesystem or `mmap` is required inside the module.
//! Source bundling reads source content through a host-supplied callback for
//! the same reason.

use std::collections::BTreeSet;
use std::io::Cursor;
use std::rc::Rc;

use serde::Serialize;
use symbolic_debuginfo::sourcebundle::SourceBundleWriter;
use symbolic_debuginfo::{Archive as DiArchive, Object as DiObject};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Convert any `Display` error into a `JsError`.
fn to_js<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}

/// Detect the object file format without a full parse. Returns the format name
/// (e.g. `elf`, `macho`, `pdb`, `breakpad`, `unknown`).
#[wasm_bindgen]
pub fn peek_format(data: &[u8]) -> String {
    DiArchive::peek(data).name().to_owned()
}

/// A parsed debug information file. May contain multiple objects (e.g. a
/// Mach-O fat binary with several architecture slices).
///
/// Owns the file bytes, so the host can drop its own copy after construction.
#[wasm_bindgen]
pub struct Archive {
    data: Rc<Vec<u8>>,
    file_format: String,
    object_count: usize,
}

#[wasm_bindgen]
impl Archive {
    /// Parse a debug information file from an in-memory byte buffer. Throws if
    /// the buffer is not a recognized object format.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<Archive, JsError> {
        let archive = DiArchive::parse(data).map_err(to_js)?;
        Ok(Archive {
            file_format: archive.file_format().name().to_owned(),
            object_count: archive.object_count(),
            data: Rc::new(data.to_vec()),
        })
    }

    /// The container file format (e.g. `elf`, `macho`, `breakpad`).
    #[wasm_bindgen(getter, js_name = fileFormat)]
    pub fn file_format(&self) -> String {
        self.file_format.clone()
    }

    /// The number of objects contained in the archive.
    #[wasm_bindgen(getter, js_name = objectCount)]
    pub fn object_count(&self) -> usize {
        self.object_count
    }

    /// All objects in the archive.
    pub fn objects(&self) -> Result<Vec<Object>, JsError> {
        let archive = DiArchive::parse(&self.data).map_err(to_js)?;
        let mut objects = Vec::with_capacity(self.object_count);
        for (index, object) in archive.objects().enumerate() {
            let object = object.map_err(to_js)?;
            objects.push(Object::new(self.data.clone(), index, &object));
        }
        Ok(objects)
    }

    /// The object at `index`, or `undefined` if out of range.
    pub fn object(&self, index: usize) -> Result<Option<Object>, JsError> {
        let archive = DiArchive::parse(&self.data).map_err(to_js)?;
        match archive.objects().nth(index) {
            Some(object) => Ok(Some(Object::new(
                self.data.clone(),
                index,
                &object.map_err(to_js)?,
            ))),
            None => Ok(None),
        }
    }
}

/// A single object (architecture slice) within an [`Archive`].
///
/// Metadata getters are cheap (cached at creation). [`Object::source_files`] and
/// [`Object::create_source_bundle`] re-read the object's debug info on demand.
#[wasm_bindgen]
pub struct Object {
    data: Rc<Vec<u8>>,
    index: usize,
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

impl Object {
    /// Snapshot an object's metadata, keeping a handle to the archive bytes for
    /// later on-demand debug-info access.
    fn new(data: Rc<Vec<u8>>, index: usize, object: &DiObject) -> Object {
        Object {
            data,
            index,
            debug_id: object.debug_id().to_string(),
            code_id: object.code_id().map(|c| c.to_string()),
            arch: object.arch().name().to_owned(),
            file_format: object.file_format().name().to_owned(),
            kind: object.kind().name().to_owned(),
            has_symbols: object.has_symbols(),
            has_debug_info: object.has_debug_info(),
            has_unwind_info: object.has_unwind_info(),
            has_sources: object.has_sources(),
        }
    }

    /// Re-parse the archive and run `f` against this object's live debug info.
    fn with_object<R, E: From<JsError>>(
        &self,
        f: impl FnOnce(&DiObject) -> Result<R, E>,
    ) -> Result<R, E> {
        let archive = DiArchive::parse(&self.data).map_err(|e| E::from(to_js(e)))?;
        let object = archive
            .objects()
            .nth(self.index)
            .ok_or_else(|| E::from(JsError::new("object index out of range")))?
            .map_err(|e| E::from(to_js(e)))?;
        f(&object)
    }
}

#[wasm_bindgen]
impl Object {
    /// The object's debug identifier (the canonical `debug_id`).
    #[wasm_bindgen(getter, js_name = debugId)]
    pub fn debug_id(&self) -> String {
        self.debug_id.clone()
    }

    /// The object's code identifier, if available.
    #[wasm_bindgen(getter, js_name = codeId)]
    pub fn code_id(&self) -> Option<String> {
        self.code_id.clone()
    }

    /// The CPU architecture name (e.g. `x86_64`, `arm64`).
    #[wasm_bindgen(getter)]
    pub fn arch(&self) -> String {
        self.arch.clone()
    }

    /// The object file format name (e.g. `elf`, `macho`).
    #[wasm_bindgen(getter, js_name = fileFormat)]
    pub fn file_format(&self) -> String {
        self.file_format.clone()
    }

    /// The object kind (e.g. `debug`, `lib`, `exe`).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.clone()
    }

    /// Whether the object contains a symbol table.
    #[wasm_bindgen(getter, js_name = hasSymbols)]
    pub fn has_symbols(&self) -> bool {
        self.has_symbols
    }

    /// Whether the object contains debug information.
    #[wasm_bindgen(getter, js_name = hasDebugInfo)]
    pub fn has_debug_info(&self) -> bool {
        self.has_debug_info
    }

    /// Whether the object contains stack-unwinding information.
    #[wasm_bindgen(getter, js_name = hasUnwindInfo)]
    pub fn has_unwind_info(&self) -> bool {
        self.has_unwind_info
    }

    /// Whether the object embeds its own source code (a source bundle).
    #[wasm_bindgen(getter, js_name = hasSources)]
    pub fn has_sources(&self) -> bool {
        self.has_sources
    }

    /// The absolute source file paths this object references, excluding
    /// compiler-synthesized `<...>` entries. Useful for `print-sources` and to
    /// decide which files to feed [`Object::create_source_bundle`].
    #[wasm_bindgen(js_name = sourceFiles)]
    pub fn source_files(&self) -> Result<Vec<String>, JsError> {
        self.with_object(|object| {
            let session = object.debug_session().map_err(to_js)?;
            let mut sources = Vec::new();
            let mut seen = BTreeSet::new();
            for file in session.files() {
                let path = file.map_err(to_js)?.abs_path_str();
                if path.starts_with('<') && path.ends_with('>') {
                    continue;
                }
                if seen.insert(path.clone()) {
                    sources.push(path);
                }
            }
            Ok(sources)
        })
    }

    /// Build a source bundle (`.src.zip`) for this object.
    ///
    /// `getSource` is called with each referenced source path and must return
    /// the file's bytes as a `Uint8Array`, or `null`/`undefined` to skip it.
    /// Reads happen lazily, so the host only provides the files it has.
    ///
    /// If `getSource` throws, or returns anything other than a `Uint8Array` /
    /// `null` / `undefined`, that error is propagated (only an explicit
    /// `null`/`undefined` skips a file) — so host read failures aren't silently
    /// turned into a partial bundle.
    ///
    /// Returns the bundle bytes, or `undefined` if no sources were bundled.
    #[wasm_bindgen(js_name = createSourceBundle)]
    pub fn create_source_bundle(
        &self,
        object_name: &str,
        get_source: &js_sys::Function,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        self.with_object(|object| {
            let mut sink = Cursor::new(Vec::new());
            let writer = SourceBundleWriter::start(&mut sink).map_err(to_js)?;
            // The provider can only signal "skip" (`None`), so a genuine callback
            // failure is recorded here and surfaced once the writer has finished.
            let mut callback_error: Option<JsValue> = None;
            let written = writer
                .write_object_with_source_provider(object, object_name, |path| {
                    let result = match get_source.call1(&JsValue::NULL, &JsValue::from_str(path)) {
                        Ok(value) => value,
                        Err(error) => {
                            callback_error.get_or_insert(error);
                            return None;
                        }
                    };
                    if result.is_null() || result.is_undefined() {
                        return None;
                    }
                    match result.dyn_into::<js_sys::Uint8Array>() {
                        Ok(bytes) => Some(Cursor::new(bytes.to_vec())),
                        Err(_) => {
                            callback_error.get_or_insert_with(|| {
                                JsError::new(&format!(
                                    "getSource(\"{path}\") must return a Uint8Array, null, or undefined"
                                ))
                                .into()
                            });
                            None
                        }
                    }
                })
                .map_err(to_js)?;
            if let Some(error) = callback_error {
                return Err(error);
            }
            Ok(written.then(|| sink.into_inner()))
        })
    }
}

/// Per-object metadata, for the legacy [`parse_debug_file`] shape.
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

/// Result of [`parse_debug_file`].
#[derive(Serialize)]
struct ArchiveInfo {
    file_format: String,
    objects: Vec<ObjectInfo>,
}

/// Parse a debug information file and return its metadata as a plain JS object.
///
/// Convenience wrapper retained for backwards compatibility; prefer the
/// [`Archive`]/[`Object`] API for new code.
#[wasm_bindgen]
pub fn parse_debug_file(data: &[u8]) -> Result<JsValue, JsError> {
    let archive = DiArchive::parse(data).map_err(to_js)?;

    let mut objects = Vec::new();
    for object in archive.objects() {
        let object = object.map_err(to_js)?;
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

    serde_wasm_bindgen::to_value(&info).map_err(to_js)
}
