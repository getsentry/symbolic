use std::io::Cursor;

use symbolic_debuginfo as di;
use wasm_bindgen::prelude::*;

use super::Object;
use crate::utils::Result;

/// A descriptor that provides information about a source file.
///
/// This descriptor is returned from `source_by_path` and friends.
///
/// This descriptor holds information that can be used to retrieve information
/// about the source file.
#[wasm_bindgen(getter_with_clone)]
pub struct SourceFileDescriptor {
    /// The type of the file the descriptor points to.
    #[wasm_bindgen(js_name = "type")]
    pub ty: String,
    /// The contents of the source file as string, if it's available.
    pub contents: Option<String>,
    /// If available returns the URL of this source.
    pub url: Option<String>,
    /// If available returns the file path of this source.
    pub path: Option<String>,
    /// The debug ID of the file if available.
    #[wasm_bindgen(js_name = debugId)]
    pub debug_id: Option<String>,
    /// The source mapping URL reference of the file.
    #[wasm_bindgen(js_name = sourceMappingUrl)]
    pub source_mapping_url: Option<String>,
}

impl<'a> From<&di::sourcebundle::SourceFileDescriptor<'a>> for SourceFileDescriptor {
    fn from(source: &di::sourcebundle::SourceFileDescriptor<'a>) -> Self {
        Self {
            ty: source.ty().to_string(),
            contents: source.contents().map(str::to_owned),
            url: source.url().map(str::to_owned),
            path: source.path().map(str::to_owned),
            debug_id: source.debug_id().map(|debug_id| debug_id.to_string()),
            source_mapping_url: source.source_mapping_url().map(str::to_owned),
        }
    }
}

/// A source file entry referenced by an object.
#[wasm_bindgen]
pub struct FileEntry {
    // We only provide the API for `abs_path_str` for now, if we ever provide more,
    // we may need to store the real `FileEntry` here.
    abs_path_str: String,
}

#[wasm_bindgen]
impl FileEntry {
    /// Absolute path to the file, including the compilation directory.
    #[wasm_bindgen(getter)]
    pub fn abs_path_str(&self) -> String {
        self.abs_path_str.clone()
    }
}

impl<'a> From<&di::FileEntry<'a>> for FileEntry {
    fn from(file: &di::FileEntry<'a>) -> Self {
        Self {
            abs_path_str: file.abs_path_str(),
        }
    }
}

#[wasm_bindgen]
pub struct SourceBundleWriter {
    inner: di::sourcebundle::SourceBundleWriter<Cursor<Vec<u8>>>,
}

#[wasm_bindgen]
impl SourceBundleWriter {
    /// Creates a bundle writer.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Self> {
        let writer = Cursor::new(Vec::new());

        Ok(Self {
            inner: di::sourcebundle::SourceBundleWriter::start(writer)?,
        })
    }

    /// Returns whether the bundle contains any files.
    #[wasm_bindgen(getter, js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// This controls if source files should be scanned for Il2cpp-specific source annotations,
    /// and the referenced C# files should be bundled up as well.
    #[wasm_bindgen(setter, js_name = collectIl2cppSources)]
    pub fn collect_il2cpp_sources(&mut self, collect_il2cpp: bool) {
        self.inner.collect_il2cpp_sources(collect_il2cpp);
    }

    /// Sets a meta data attribute of the bundle.
    ///
    /// Attributes are flushed to the bundle when it is finished. Thus, they can be retrieved or
    /// changed at any time before flushing the writer.
    ///
    /// If the attribute was set before, the prior value is returned.
    #[wasm_bindgen(js_name = setAttribute)]
    pub fn set_attribute(&mut self, key: String, value: String) -> Option<String> {
        self.inner.set_attribute(key, value)
    }

    /// Removes a meta data attribute of the bundle.
    ///
    /// If the attribute was set, the last value is returned.
    #[wasm_bindgen(js_name = removeAttribute)]
    pub fn remove_attribute(&mut self, key: &str) -> Option<String> {
        self.inner.remove_attribute(key)
    }

    /// Returns the value of a meta data attribute.
    pub fn attribute(&mut self, key: &str) -> Option<String> {
        self.inner.attribute(key).map(str::to_owned)
    }

    /// Determines whether a file at the given path has been added already.
    #[wasm_bindgen(js_name = hasFile)]
    pub fn has_file(&self, path: &str) -> bool {
        self.inner.has_file(path)
    }

    /// Writes a single object into the bundle.
    ///
    /// Loads source files by invoking the `provider` with the file path.
    /// Before a file is written the `filter` is invoked which can return `false` to skip a file.
    ///
    /// This finishes the source bundle and returns its contents.
    #[wasm_bindgen(js_name = writeObject)]
    pub fn write_object(
        self,
        object: &Object,
        object_name: &str,
        filter: &js_sys::Function,
        provider: &js_sys::Function,
    ) -> Result<Option<Vec<u8>>> {
        let written = self.inner.write_object_with_filter_and_provider(
            object.inner.get(),
            object_name,
            |file, source| {
                if filter.is_null_or_undefined() {
                    return true;
                };

                let file = JsValue::from(FileEntry::from(file));
                let source = source
                    .as_ref()
                    .map(SourceFileDescriptor::from)
                    .map(JsValue::from)
                    .unwrap_or(JsValue::UNDEFINED);

                filter
                    .call2(&JsValue::UNDEFINED, &file, &source)
                    .unwrap_throw()
                    .is_truthy()
            },
            |path| {
                let value = provider
                    .call1(&JsValue::UNDEFINED, &JsValue::from_str(path))
                    .unwrap_throw();

                if value.is_null_or_undefined() {
                    return None;
                }

                Some(Cursor::new(js_sys::Uint8Array::new(&value).to_vec()))
            },
        );

        written
            .map(|(written, w)| written.then_some(w.into_inner()))
            .map_err(Into::into)
    }
}
