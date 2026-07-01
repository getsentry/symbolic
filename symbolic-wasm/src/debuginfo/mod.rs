//! Exposes `symbolic_debuginfo` to WASM.

use symbolic_common::{ByteView, SelfCell};
use symbolic_debuginfo as di;
use wasm_bindgen::prelude::*;

use crate::utils::{self, Error, Result};

pub mod sourcebundle;

use sourcebundle::{FileEntry, SourceFileDescriptor};

/// A generic archive that can contain one or more object files.
#[wasm_bindgen]
pub struct Archive {
    inner: SelfCell<ByteView<'static>, di::Archive<'static>>,
}

#[wasm_bindgen]
impl Archive {
    /// Parse a debug information file from an in-memory byte buffer.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8]) -> Result<Archive> {
        let b = ByteView::from_vec(data.to_vec());
        Ok(Self {
            inner: SelfCell::try_new(b, |p| unsafe { di::Archive::parse(&*p) })?,
        })
    }

    /// Tries to infer the object archive type from the start of the given buffer.
    ///
    /// Returns `None` if the [`Self::file_format`] cannot be identified.
    pub fn peek(data: &[u8]) -> Option<String> {
        match di::Archive::peek(data) {
            di::FileFormat::Unknown => None,
            ff => Some(ff.to_string()),
        }
    }

    /// The container format of this file (e.g. `elf`, `macho`, `breakpad`).
    #[wasm_bindgen(getter, js_name = fileFormat)]
    pub fn file_format(&self) -> String {
        self.inner.get().file_format().to_string()
    }

    /// The number of objects contained in the archive.
    #[wasm_bindgen(getter, js_name = objectCount)]
    pub fn object_count(&self) -> usize {
        self.inner.get().object_count()
    }

    /// Returns a list of all objects contained in this archive.
    pub fn objects(&self) -> Result<Vec<Object>> {
        let inner = self.inner.get();
        inner
            .objects()
            .map(|object| {
                object
                    .map(|object| Object {
                        // SAFETY: `object` is directly derived from `self.inner` and
                        // only borrows data from the same `ByteView`.
                        inner: unsafe { utils::derived_from_cell!(Object, self.inner, object) },
                    })
                    .map_err(Error::from)
            })
            .collect()
    }
}

/// A generic object file providing uniform access to various file formats.
#[wasm_bindgen(js_name = ObjectFile)]
pub struct Object {
    inner: SelfCell<ByteView<'static>, di::Object<'static>>,
}

#[wasm_bindgen(js_class = ObjectFile)]
impl Object {
    /// The object's debug identifier (the canonical `debug_id`).
    #[wasm_bindgen(getter, js_name = debugId)]
    pub fn debug_id(&self) -> String {
        self.inner.get().debug_id().to_string()
    }

    /// The object's code identifier, if available.
    #[wasm_bindgen(getter, js_name = codeId)]
    pub fn code_id(&self) -> Option<String> {
        self.inner
            .get()
            .code_id()
            .map(|code_id| code_id.to_string())
    }

    /// The CPU architecture name (e.g. `x86_64`, `arm64`).
    #[wasm_bindgen(getter)]
    pub fn arch(&self) -> String {
        self.inner.get().arch().to_string()
    }

    /// The object file format name (e.g. `elf`, `macho`).
    #[wasm_bindgen(getter, js_name = fileFormat)]
    pub fn file_format(&self) -> String {
        self.inner.get().file_format().to_string()
    }

    /// The object kind (e.g. `debug`, `lib`, `exe`).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.inner.get().kind().to_string()
    }

    /// Whether the object contains a symbol table.
    #[wasm_bindgen(getter, js_name = hasSymbols)]
    pub fn has_symbols(&self) -> bool {
        self.inner.get().has_symbols()
    }

    /// Whether the object contains debug information.
    #[wasm_bindgen(getter, js_name = hasDebugInfo)]
    pub fn has_debug_info(&self) -> bool {
        self.inner.get().has_debug_info()
    }

    /// Whether the object contains stack-unwinding information.
    #[wasm_bindgen(getter, js_name = hasUnwindInfo)]
    pub fn has_unwind_info(&self) -> bool {
        self.inner.get().has_unwind_info()
    }

    /// Whether the object embeds its own source code (a source bundle).
    #[wasm_bindgen(getter, js_name = hasSources)]
    pub fn has_sources(&self) -> bool {
        self.inner.get().has_sources()
    }

    /// Returns a debug session that provides access to debugging information
    /// stored in this object, in particular the source files it references.
    #[wasm_bindgen(js_name = debugSession)]
    pub fn debug_session(&self) -> Result<DebugSession> {
        let session = self.inner.get().debug_session()?;
        Ok(DebugSession {
            // SAFETY: `session` is directly derived from `self.inner` and only
            // borrows data from the same `ByteView`.
            inner: unsafe { utils::derived_from_cell!(ObjectDebugSession, self.inner, session) },
        })
    }

    /// Narrows this object to a Windows PE image, if it is one.
    ///
    /// Returns `undefined` for non-PE objects. The returned [`PeFile`] exposes
    /// PE-specific operations (such as [`PeFile::embedded_ppdb`]) that have no
    /// equivalent on other object formats, mirroring the `Object::Pe` variant of
    /// the underlying `symbolic_debuginfo` types.
    #[wasm_bindgen(js_name = asPe)]
    pub fn as_pe(self) -> Option<PeFile> {
        match self.inner.get() {
            di::Object::Pe(_) => Some(PeFile { inner: self.inner }),
            _ => None,
        }
    }
}

/// A Windows PE image (an executable or a managed/.NET assembly).
///
/// Obtain one by narrowing an [`Object`] via [`Object::as_pe`]. This exposes the
/// PE-specific surface of `symbolic_debuginfo`'s `PeObject`.
#[wasm_bindgen(js_name = PeFile)]
pub struct PeFile {
    // Since we don't have a good way of extracting a `PeObject` from the
    // `SelfCell`, we just rely on the invariant to always be constructed
    // with a valid `PeObject`.
    inner: SelfCell<ByteView<'static>, di::Object<'static>>,
}

#[wasm_bindgen(js_class = PeFile)]
impl PeFile {
    fn pe(&self) -> &di::pe::PeObject<'_> {
        match self.inner.get() {
            di::Object::Pe(pe) => pe,
            _ => unreachable!("inner must always be a pe object"),
        }
    }

    /// Extracts the embedded Portable PDB from this PE, if present.
    ///
    /// Some Windows PE images (managed/.NET assemblies) embed their Portable
    /// PDB debug companion directly in the executable (debug directory entry
    /// type 17, deflate-compressed). This decompresses and returns those bytes,
    /// which are themselves a standalone Portable PDB debug information file
    /// that can be parsed (e.g. via [`Archive`]) and uploaded independently.
    ///
    /// Returns `undefined` when this PE has no embedded Portable PDB.
    #[wasm_bindgen(js_name = embeddedPpdb)]
    pub fn embedded_ppdb(&self) -> Result<Option<Vec<u8>>> {
        let Some(ppdb) = self.pe().embedded_ppdb()? else {
            return Ok(None);
        };
        // Decompress into a growable buffer rather than pre-allocating from
        // `ppdb.get_size()`: the uncompressed size is read verbatim from the
        // (untrusted) PE debug-directory header, so a crafted file could claim a
        // huge size and trigger an oversized speculative allocation. Letting the
        // deflate decoder grow the buffer bounds memory to the actual output.
        let mut buf = Vec::new();
        ppdb.decompress_to(&mut buf)?;
        Ok(Some(buf))
    }
}

/// A debug session that provides access to an object's debugging information.
///
/// In particular, this enumerates the source files referenced by the object and
/// resolves their contents (when embedded) or source links.
#[wasm_bindgen]
pub struct DebugSession {
    inner: SelfCell<ByteView<'static>, di::ObjectDebugSession<'static>>,
}

#[wasm_bindgen]
impl DebugSession {
    /// Returns all source files referenced by the object.
    ///
    /// Note that this only lists referenced files; use [`Self::source_by_path`]
    /// to retrieve a file's embedded contents or source link.
    pub fn files(&self) -> Result<Vec<FileEntry>> {
        self.inner
            .get()
            .files()
            .map(|file| file.map(|file| FileEntry::from(&file)).map_err(Error::from))
            .collect()
    }

    /// Looks up a source file by its full, canonicalized path.
    ///
    /// Returns the descriptor (embedded contents or a source link) if the path
    /// is referenced by the object, otherwise `undefined`.
    #[wasm_bindgen(js_name = sourceByPath)]
    pub fn source_by_path(&self, path: &str) -> Result<Option<SourceFileDescriptor>> {
        Ok(self
            .inner
            .get()
            .source_by_path(path)?
            .as_ref()
            .map(SourceFileDescriptor::from))
    }
}
