//! Support for Source Bundles, a proprietary archive containing source code.
//!
//! This module defines the [`SourceBundle`] type. Since not all object file containers specify a
//! standardized way to inline sources into debug information, this can be used to associate source
//! contents to debug files.
//!
//! Source bundles are ZIP archives with a well-defined internal structure. Most importantly, they
//! contain source files in a nested directory structure. Additionally, there is meta data
//! associated to every source file, which allows to store additional properties, such as the
//! original file system path, a web URL, and custom headers.
//!
//! The internal structure is as follows:
//!
//! ```txt
//! manifest.json
//! files/
//!   file1.txt
//!   subfolder/
//!     file2.txt
//! ```
//!
//! `SourceBundle` implements the [`ObjectLike`] trait. When created from another object, it carries
//! over its meta data, such as the [`debug_id`] or [`code_id`]. However, source bundles never store
//! symbols or debug information. To obtain sources or iterate files stored in this source bundle,
//! use [`SourceBundle::debug_session`].
//!
//! Source bundles can be created manually or by converting any `ObjectLike` using
//! [`SourceBundleWriter`].
//!
//! [`ObjectLike`]: ../trait.ObjectLike.html
//! [`SourceBundle`]: struct.SourceBundle.html
//! [`debug_id`]: struct.SourceBundle.html#method.debug_id
//! [`code_id`]: struct.SourceBundle.html#method.code_id
//! [`SourceBundle::debug_session`]: struct.SourceBundle.html#method.debug_session
//! [`SourceBundleWriter`]: struct.SourceBundleWriter.html

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::Path;
use std::sync::Arc;

use lazycell::LazyCell;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::{write::FileOptions, ZipWriter};

use symbolic_common::{Arch, AsSelf, CodeId, DebugId};

use crate::base::*;
use crate::{DebugSession, ObjectKind, ObjectLike};

/// Magic bytes of a source bundle. They are prepended to the ZIP file.
static BUNDLE_MAGIC: [u8; 4] = *b"SYSB";

/// Version of the bundle and manifest format.
static BUNDLE_VERSION: u32 = 2;

/// Relative path to the manifest file in the bundle file.
static MANIFEST_PATH: &str = "manifest.json";

/// Path at which files will be written into the bundle.
static FILES_PATH: &str = "files";

lazy_static::lazy_static! {
    static ref SANE_PATH_RE: Regex = Regex::new(r#":?[/\\]+"#).unwrap();
}

/// The error type for [`SourceBundleError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceBundleErrorKind {
    /// The source bundle container is damanged.
    BadZip,

    /// An error when reading/writing the manifest.
    BadManifest,

    /// The `Object` contains invalid data and cannot be converted.
    BadDebugFile,

    /// Generic error when writing a source bundle, most likely IO.
    WriteFailed,
}

impl fmt::Display for SourceBundleErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadZip => write!(f, "malformed zip archive"),
            Self::BadManifest => write!(f, "failed to read/write source bundle manifest"),
            Self::BadDebugFile => write!(f, "malformed debug info file"),
            Self::WriteFailed => write!(f, "failed to write source bundle"),
        }
    }
}

/// An error returned when handling [`SourceBundle`](struct.SourceBundle.html).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct SourceBundleError {
    kind: SourceBundleErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl SourceBundleError {
    /// Creates a new SourceBundle error from a known kind of error as well as an arbitrary error
    /// payload.
    ///
    /// This function is used to generically create source bundle errors which do not originate from
    /// `symbolic` itself. The `source` argument is an arbitrary payload which will be contained in
    /// this [`SourceBundleError`].
    pub fn new<E>(kind: SourceBundleErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`SourceBundleErrorKind`] for this error.
    pub fn kind(&self) -> SourceBundleErrorKind {
        self.kind
    }
}

impl From<SourceBundleErrorKind> for SourceBundleError {
    fn from(kind: SourceBundleErrorKind) -> Self {
        Self { kind, source: None }
    }
}

/// Trims matching suffices of a string in-place.
fn trim_end_matches<F>(string: &mut String, pat: F)
where
    F: FnMut(char) -> bool,
{
    let cutoff = string.trim_end_matches(pat).len();
    string.truncate(cutoff);
}

/// The type of a [`SourceFileInfo`](struct.SourceFileInfo.html).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFileType {
    /// Regular source file.
    Source,

    /// Minified source code.
    MinifiedSource,

    /// JavaScript sourcemap.
    SourceMap,

    /// Indexed JavaScript RAM bundle.
    IndexedRamBundle,
}

/// Meta data information of a file in a [`SourceBundle`](struct.SourceBundle.html).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceFileInfo {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ty: Option<SourceFileType>,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    path: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    url: String,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
}

impl SourceFileInfo {
    /// Creates default file information.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the type of the source file.
    pub fn ty(&self) -> Option<SourceFileType> {
        self.ty
    }

    /// Sets the type of the source file.
    pub fn set_ty(&mut self, ty: SourceFileType) {
        self.ty = Some(ty);
    }

    /// Returns the absolute file system path of this file.
    pub fn path(&self) -> Option<&str> {
        match self.path.as_str() {
            "" => None,
            path => Some(path),
        }
    }

    /// Sets the absolute file system path of this file.
    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }

    /// Returns the web URL that of this file.
    pub fn url(&self) -> Option<&str> {
        match self.url.as_str() {
            "" => None,
            url => Some(url),
        }
    }

    /// Sets the web URL of this file.
    pub fn set_url(&mut self, url: String) {
        self.url = url;
    }

    /// Iterates over all attributes represented as headers.
    pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Retrieves the specified header, if it exists.
    pub fn header(&self, header: &str) -> Option<&str> {
        self.headers.get(header).map(String::as_str)
    }

    /// Adds a custom attribute following header conventions.
    pub fn add_header(&mut self, header: String, value: String) {
        self.headers.insert(header, value);
    }

    /// Returns `true` if this instance does not carry any information.
    pub fn is_empty(&self) -> bool {
        self.path.is_empty() && self.ty.is_none() && self.headers.is_empty()
    }
}

/// Version number of a [`SourceBundle`](struct.SourceBundle.html).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SourceBundleVersion(pub u32);

impl SourceBundleVersion {
    /// Creates a new source bundle version.
    pub fn new(version: u32) -> Self {
        Self(version)
    }

    /// Determines whether this version can be handled.
    ///
    /// This will return `false`, if the version is newer than what is supported by this library
    /// version.
    pub fn is_valid(self) -> bool {
        self.0 <= BUNDLE_VERSION
    }

    /// Returns whether the given bundle is at the latest supported versino.
    pub fn is_latest(self) -> bool {
        self.0 == BUNDLE_VERSION
    }
}

impl Default for SourceBundleVersion {
    fn default() -> Self {
        Self(BUNDLE_VERSION)
    }
}

/// Binary header of the source bundle archive.
///
/// This header precedes the ZIP archive. It is used to detect these files on the file system.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct SourceBundleHeader {
    /// Magic bytes header.
    pub magic: [u8; 4],

    /// Version of the bundle.
    pub version: u32,
}

impl SourceBundleHeader {
    fn as_bytes(&self) -> &[u8] {
        let ptr = self as *const Self as *const u8;
        unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<Self>()) }
    }
}

impl Default for SourceBundleHeader {
    fn default() -> Self {
        SourceBundleHeader {
            magic: BUNDLE_MAGIC,
            version: BUNDLE_VERSION,
        }
    }
}

/// Manifest of a [`SourceBundle`] containing information on its contents.
///
/// [`SourceBundle`]: struct.SourceBundle.html
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct SourceBundleManifest {
    /// Descriptors for all files in this bundle.
    #[serde(default)]
    pub files: BTreeMap<String, SourceFileInfo>,

    /// Arbitrary attributes to include in the bundle.
    #[serde(flatten)]
    pub attributes: BTreeMap<String, String>,
}

/// A bundle of source code files.
///
/// To create a source bundle, see [`SourceBundleWriter`]. For more information, see the [module
/// level documentation].
///
/// [`SourceBundleWriter`]: struct.SourceBundleWriter.html
/// [module level documentation]: index.html
pub struct SourceBundle<'data> {
    manifest: Arc<SourceBundleManifest>,
    archive: Arc<Mutex<zip::read::ZipArchive<std::io::Cursor<&'data [u8]>>>>,
    data: &'data [u8],
}

impl fmt::Debug for SourceBundle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceBundle")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .field("has_sources", &self.has_sources())
            .field("is_malformed", &self.is_malformed())
            .finish()
    }
}

impl<'data> SourceBundle<'data> {
    /// Tests whether the buffer could contain a `SourceBundle`.
    pub fn test(bytes: &[u8]) -> bool {
        bytes.starts_with(&BUNDLE_MAGIC)
    }

    /// Tries to parse a `SourceBundle` from the given slice.
    pub fn parse(data: &'data [u8]) -> Result<SourceBundle<'data>, SourceBundleError> {
        let mut archive = zip::read::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadZip, e))?;
        let manifest_file = archive
            .by_name("manifest.json")
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadZip, e))?;
        let manifest = serde_json::from_reader(manifest_file)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadManifest, e))?;
        Ok(SourceBundle {
            manifest: Arc::new(manifest),
            archive: Arc::new(Mutex::new(archive)),
            data,
        })
    }

    /// Returns the version of this source bundle format.
    pub fn version(&self) -> SourceBundleVersion {
        SourceBundleVersion(BUNDLE_VERSION)
    }

    /// The container file format, which is always `FileFormat::SourceBundle`.
    pub fn file_format(&self) -> FileFormat {
        FileFormat::SourceBundle
    }

    /// The code identifier of this object.
    ///
    /// This is only set if the source bundle was created from an [`ObjectLike`]. It can also be set
    /// in the [`SourceBundleWriter`] by setting the `"code_id"` attribute.
    ///
    /// [`ObjectLike`]: ../trait.ObjectLike.html
    /// [`SourceBundleWriter`]: struct.SourceBundleWriter.html
    pub fn code_id(&self) -> Option<CodeId> {
        self.manifest
            .attributes
            .get("code_id")
            .and_then(|x| x.parse().ok())
    }

    /// The code identifier of this object.
    ///
    /// This is only set if the source bundle was created from an [`ObjectLike`]. It can also be set
    /// in the [`SourceBundleWriter`] by setting the `"debug_id"` attribute.
    ///
    /// [`ObjectLike`]: ../trait.ObjectLike.html
    /// [`SourceBundleWriter`]: struct.SourceBundleWriter.html
    pub fn debug_id(&self) -> DebugId {
        self.manifest
            .attributes
            .get("debug_id")
            .and_then(|x| x.parse().ok())
            .unwrap_or_default()
    }

    /// The debug file name of this object.
    ///
    /// This is only set if the source bundle was created from an [`ObjectLike`]. It can also be set
    /// in the [`SourceBundleWriter`] by setting the `"object_name"` attribute.
    ///
    /// [`ObjectLike`]: ../trait.ObjectLike.html
    /// [`SourceBundleWriter`]: struct.SourceBundleWriter.html
    pub fn name(&self) -> Option<&str> {
        self.manifest
            .attributes
            .get("object_name")
            .map(|x| x.as_str())
    }

    /// The CPU architecture of this object.
    ///
    /// This is only set if the source bundle was created from an [`ObjectLike`]. It can also be set
    /// in the [`SourceBundleWriter`] by setting the `"arch"` attribute.
    ///
    /// [`ObjectLike`]: ../trait.ObjectLike.html
    /// [`SourceBundleWriter`]: struct.SourceBundleWriter.html
    pub fn arch(&self) -> Arch {
        self.manifest
            .attributes
            .get("arch")
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }

    /// The kind of this object.
    ///
    /// Because source bundles do not contain real objects this is always `ObjectKind::None`.
    fn kind(&self) -> ObjectKind {
        ObjectKind::Sources
    }

    /// The address at which the image prefers to be loaded into memory.
    ///
    /// Because source bundles do not contain this information is always `0`.
    pub fn load_address(&self) -> u64 {
        0
    }

    /// Determines whether this object exposes a public symbol table.
    ///
    /// Source bundles never have symbols.
    pub fn has_symbols(&self) -> bool {
        false
    }

    /// Returns an iterator over symbols in the public symbol table.
    pub fn symbols(&self) -> SourceBundleSymbolIterator<'data> {
        std::iter::empty()
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbols().collect()
    }

    /// Determines whether this object contains debug information.
    ///
    /// Source bundles never have debug info.
    pub fn has_debug_info(&self) -> bool {
        false
    }

    /// Constructs a debugging session.
    ///
    /// A debugging session loads certain information from the object file and creates caches for
    /// efficient access to various records in the debug information. Since this can be quite a
    /// costly process, try to reuse the debugging session as long as possible.
    pub fn debug_session(&self) -> Result<SourceBundleDebugSession<'data>, SourceBundleError> {
        Ok(SourceBundleDebugSession {
            manifest: self.manifest.clone(),
            archive: self.archive.clone(),
            files_by_path: LazyCell::new(),
        })
    }

    /// Determines whether this object contains stack unwinding information.
    pub fn has_unwind_info(&self) -> bool {
        false
    }

    /// Determines whether this object contains embedded source.
    pub fn has_sources(&self) -> bool {
        true
    }

    /// Determines whether this object is malformed and was only partially parsed
    pub fn is_malformed(&self) -> bool {
        false
    }

    /// Returns the raw data of the source bundle.
    pub fn data(&self) -> &'data [u8] {
        self.data
    }

    /// Returns true if this source bundle contains no source code.
    pub fn is_empty(&self) -> bool {
        self.manifest.files.is_empty()
    }
}

impl<'slf, 'data: 'slf> AsSelf<'slf> for SourceBundle<'data> {
    type Ref = SourceBundle<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

impl<'data> Parse<'data> for SourceBundle<'data> {
    type Error = SourceBundleError;

    fn parse(data: &'data [u8]) -> Result<Self, Self::Error> {
        SourceBundle::parse(data)
    }

    fn test(data: &'data [u8]) -> bool {
        SourceBundle::test(data)
    }
}

impl<'data: 'object, 'object> ObjectLike<'data, 'object> for SourceBundle<'data> {
    type Error = SourceBundleError;
    type Session = SourceBundleDebugSession<'data>;
    type SymbolIterator = SourceBundleSymbolIterator<'data>;

    fn file_format(&self) -> FileFormat {
        self.file_format()
    }

    fn code_id(&self) -> Option<CodeId> {
        self.code_id()
    }

    fn debug_id(&self) -> DebugId {
        self.debug_id()
    }

    fn arch(&self) -> Arch {
        self.arch()
    }

    fn kind(&self) -> ObjectKind {
        self.kind()
    }

    fn load_address(&self) -> u64 {
        self.load_address()
    }

    fn has_symbols(&self) -> bool {
        self.has_symbols()
    }

    fn symbol_map(&self) -> SymbolMap<'data> {
        self.symbol_map()
    }

    fn symbols(&self) -> Self::SymbolIterator {
        self.symbols()
    }

    fn has_debug_info(&self) -> bool {
        self.has_debug_info()
    }

    fn debug_session(&self) -> Result<Self::Session, Self::Error> {
        self.debug_session()
    }

    fn has_unwind_info(&self) -> bool {
        self.has_unwind_info()
    }

    fn has_sources(&self) -> bool {
        self.has_sources()
    }

    fn is_malformed(&self) -> bool {
        self.is_malformed()
    }
}

/// An iterator yielding symbols from a source bundle.
pub type SourceBundleSymbolIterator<'data> = std::iter::Empty<Symbol<'data>>;

/// Debug session for SourceBundle objects.
pub struct SourceBundleDebugSession<'data> {
    manifest: Arc<SourceBundleManifest>,
    archive: Arc<Mutex<zip::read::ZipArchive<std::io::Cursor<&'data [u8]>>>>,
    files_by_path: LazyCell<HashMap<String, String>>,
}

impl<'data> SourceBundleDebugSession<'data> {
    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> SourceBundleFileIterator<'_> {
        SourceBundleFileIterator {
            files: self.manifest.files.values(),
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> SourceBundleFunctionIterator<'_> {
        std::iter::empty()
    }

    /// Create a reverse mapping of source paths to ZIP paths.
    fn get_files_by_path(&self) -> HashMap<String, String> {
        let files = &self.manifest.files;
        let mut files_by_path = HashMap::with_capacity(files.len());

        for (zip_path, file_info) in files {
            if !file_info.path.is_empty() {
                files_by_path.insert(file_info.path.clone(), zip_path.clone());
            }
        }

        files_by_path
    }

    /// Get the path of a file in this bundle by its logical path.
    fn zip_path_by_source_path(&self, path: &str) -> Option<&str> {
        self.files_by_path
            .borrow_with(|| self.get_files_by_path())
            .get(path)
            .map(|zip_path| zip_path.as_str())
    }

    /// Get source by the path of a file in the bundle.
    fn source_by_zip_path(&self, zip_path: &str) -> Result<Option<String>, SourceBundleError> {
        let mut archive = self.archive.lock();
        let mut file = archive
            .by_name(zip_path)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadZip, e))?;
        let mut source_content = String::new();

        file.read_to_string(&mut source_content)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadZip, e))?;
        Ok(Some(source_content))
    }

    /// Looks up a file's source contents by its full canonicalized path.
    ///
    /// The given path must be canonicalized.
    pub fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, SourceBundleError> {
        let zip_path = match self.zip_path_by_source_path(path) {
            Some(zip_path) => zip_path,
            None => return Ok(None),
        };

        self.source_by_zip_path(zip_path)
            .map(|opt| opt.map(Cow::Owned))
    }
}

impl<'data, 'session> DebugSession<'session> for SourceBundleDebugSession<'data> {
    type Error = SourceBundleError;
    type FunctionIterator = SourceBundleFunctionIterator<'session>;
    type FileIterator = SourceBundleFileIterator<'session>;

    fn functions(&'session self) -> Self::FunctionIterator {
        self.functions()
    }

    fn files(&'session self) -> Self::FileIterator {
        self.files()
    }

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error> {
        self.source_by_path(path)
    }
}

/// An iterator over source files in a SourceBundle object.
pub struct SourceBundleFileIterator<'s> {
    files: std::collections::btree_map::Values<'s, String, SourceFileInfo>,
}

impl<'s> Iterator for SourceBundleFileIterator<'s> {
    type Item = Result<FileEntry<'s>, SourceBundleError>;

    fn next(&mut self) -> Option<Self::Item> {
        let source_file = self.files.next()?;
        Some(Ok(FileEntry {
            compilation_dir: &[],
            info: FileInfo::from_path(source_file.path.as_bytes()),
        }))
    }
}

/// An iterator over functions in a SourceBundle object.
pub type SourceBundleFunctionIterator<'s> =
    std::iter::Empty<Result<Function<'s>, SourceBundleError>>;

impl SourceBundleManifest {
    /// Creates a new, empty manifest.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Generates a normalized path for a file in the bundle.
///
/// This removes all special characters. The path in the bundle will mostly resemble the original
/// path, except for unsupported components.
fn sanitize_bundle_path(path: &str) -> String {
    let mut sanitized = SANE_PATH_RE.replace_all(path, "/").into_owned();
    if sanitized.starts_with('/') {
        sanitized.remove(0);
    }
    sanitized
}

/// Writer to create [`SourceBundles`].
///
/// Writers can either [create a new file] or be created from an [existing file]. Then, use
/// [`add_file`] to add files and finally call [`finish`] to flush the archive to
/// the underlying writer.
///
/// Note that dropping the writer
///
/// ```no_run
/// # use std::fs::File;
/// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, SourceFileInfo};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut bundle = SourceBundleWriter::create("bundle.zip")?;
///
/// // Add file called "foo.txt"
/// let file = File::open("my_file.txt")?;
/// bundle.add_file("foo.txt", file, SourceFileInfo::default())?;
///
/// // Flush the bundle to disk
/// bundle.finish()?;
/// # Ok(()) }
/// ```
///
/// [`SourceBundles`]: struct.SourceBundle.html
/// [create a new file]: struct.SourceBundleWriter.html#method.create
/// [existing file]: struct.SourceBundleWriter.html#method.new
/// [`add_file`]: struct.SourceBundleWriter.html#method.add_file
/// [`finish`]: struct.SourceBundleWriter.html#method.finish
pub struct SourceBundleWriter<W>
where
    W: Seek + Write,
{
    manifest: SourceBundleManifest,
    writer: ZipWriter<W>,
    collect_il2cpp: bool,
}

impl<W> SourceBundleWriter<W>
where
    W: Seek + Write,
{
    /// Creates a bundle writer on the given file.
    pub fn start(mut writer: W) -> Result<Self, SourceBundleError> {
        let header = SourceBundleHeader::default();
        writer
            .write_all(header.as_bytes())
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;

        Ok(SourceBundleWriter {
            manifest: SourceBundleManifest::new(),
            writer: ZipWriter::new(writer),
            collect_il2cpp: false,
        })
    }

    /// Returns whether the bundle contains any files.
    pub fn is_empty(&self) -> bool {
        self.manifest.files.is_empty()
    }

    /// This controls if source files should be scanned for Il2cpp-specific source annotations,
    /// and the referenced C# files should be bundled ups as well.
    pub fn collect_il2cpp_sources(&mut self, collect_il2cpp: bool) {
        self.collect_il2cpp = collect_il2cpp;
    }

    /// Sets a meta data attribute of the bundle.
    ///
    /// Attributes are flushed to the bundle when it is [finished]. Thus, they can be retrieved or
    /// changed at any time before flushing the writer.
    ///
    /// If the attribute was set before, the prior value is returned.
    ///
    /// [finished]: struct.SourceBundleWriter.html#method.remove_attribute
    pub fn set_attribute<K, V>(&mut self, key: K, value: V) -> Option<String>
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.manifest.attributes.insert(key.into(), value.into())
    }

    /// Removes a meta data attribute of the bundle.
    ///
    /// If the attribute was set, the last value is returned.
    pub fn remove_attribute<K>(&mut self, key: K) -> Option<String>
    where
        K: AsRef<str>,
    {
        self.manifest.attributes.remove(key.as_ref())
    }

    /// Returns the value of a meta data attribute.
    pub fn attribute<K>(&mut self, key: K) -> Option<&str>
    where
        K: AsRef<str>,
    {
        self.manifest
            .attributes
            .get(key.as_ref())
            .map(String::as_str)
    }

    /// Determines whether a file at the given path has been added already.
    pub fn has_file<S>(&self, path: S) -> bool
    where
        S: AsRef<str>,
    {
        let full_path = &self.file_path(path.as_ref());
        self.manifest.files.contains_key(full_path)
    }

    /// Adds a file and its info to the bundle.
    ///
    /// Multiple files can be added at the same path. For the first duplicate, a counter will be
    /// appended to the file name. Any subsequent duplicate increases that counter. For example:
    ///
    /// ```no_run
    /// # use std::fs::File;
    /// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, SourceFileInfo};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut bundle = SourceBundleWriter::create("bundle.zip")?;
    ///
    /// // Add file at "foo.txt"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, SourceFileInfo::default())?;
    /// assert!(bundle.has_file("foo.txt"));
    ///
    /// // Add duplicate at "foo.txt.1"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, SourceFileInfo::default())?;
    /// assert!(bundle.has_file("foo.txt.1"));
    /// # Ok(()) }
    /// ```
    ///
    /// Returns `Ok(true)` if the file was successfully added, or `Ok(false)` if the file aready
    /// existed. Otherwise, an error is returned if writing the file fails.
    pub fn add_file<S, R>(
        &mut self,
        path: S,
        mut file: R,
        info: SourceFileInfo,
    ) -> Result<(), SourceBundleError>
    where
        S: AsRef<str>,
        R: Read,
    {
        let full_path = self.file_path(path.as_ref());
        let unique_path = self.unique_path(full_path);

        self.writer
            .start_file(unique_path.clone(), FileOptions::default())
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;
        std::io::copy(&mut file, &mut self.writer)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;

        self.manifest.files.insert(unique_path, info);
        Ok(())
    }

    /// Writes a single object into the bundle.
    ///
    /// Returns `Ok(true)` if any source files were added to the bundle, or `Ok(false)` if no
    /// sources could be resolved. Otherwise, an error is returned if writing the bundle fails.
    ///
    /// This finishes the source bundle and flushes the underlying writer.
    pub fn write_object<'data, 'object, O, E>(
        self,
        object: &'object O,
        object_name: &str,
    ) -> Result<bool, SourceBundleError>
    where
        O: ObjectLike<'data, 'object, Error = E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.write_object_with_filter(object, object_name, |_| true)
    }

    /// Writes a single object into the bundle.
    ///
    /// Returns `Ok(true)` if any source files were added to the bundle, or `Ok(false)` if no
    /// sources could be resolved. Otherwise, an error is returned if writing the bundle fails.
    ///
    /// This finishes the source bundle and flushes the underlying writer.
    ///
    /// Before a file is written a callback is invoked which can return `false` to skip a file.
    pub fn write_object_with_filter<'data, 'object, O, E, F>(
        mut self,
        object: &'object O,
        object_name: &str,
        mut filter: F,
    ) -> Result<bool, SourceBundleError>
    where
        O: ObjectLike<'data, 'object, Error = E>,
        E: std::error::Error + Send + Sync + 'static,
        F: FnMut(&FileEntry) -> bool,
    {
        let mut files_handled = BTreeSet::new();
        let mut referenced_files = BTreeSet::new();

        let session = object
            .debug_session()
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadDebugFile, e))?;

        self.set_attribute("arch", object.arch().to_string());
        self.set_attribute("debug_id", object.debug_id().to_string());
        self.set_attribute("object_name", object_name);
        if let Some(code_id) = object.code_id() {
            self.set_attribute("code_id", code_id.to_string());
        }

        for file_result in session.files() {
            let file = file_result
                .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadDebugFile, e))?;
            let filename = file.abs_path_str();

            if files_handled.contains(&filename) {
                continue;
            }

            let source = if (filename.starts_with('<') && filename.ends_with('>')) || !filter(&file)
            {
                None
            } else {
                std::fs::read(&filename).ok()
            };

            if let Some(source) = source {
                let bundle_path = sanitize_bundle_path(&filename);
                let mut info = SourceFileInfo::new();
                info.set_ty(SourceFileType::Source);
                info.set_path(filename.clone());

                if self.collect_il2cpp {
                    collect_il2cpp_sources(&source, &mut referenced_files);
                }

                self.add_file(bundle_path, source.as_slice(), info)?;
            }

            files_handled.insert(filename);
        }

        for filename in referenced_files {
            if files_handled.contains(&filename) {
                continue;
            }

            if let Some(source) = File::open(&filename).ok().map(BufReader::new) {
                let bundle_path = sanitize_bundle_path(&filename);
                let mut info = SourceFileInfo::new();
                info.set_ty(SourceFileType::Source);
                info.set_path(filename.clone());

                self.add_file(bundle_path, source, info)?;
            }
        }

        let is_empty = self.is_empty();
        self.finish()?;

        Ok(!is_empty)
    }

    /// Writes the manifest to the bundle and flushes the underlying file handle.
    pub fn finish(mut self) -> Result<(), SourceBundleError> {
        self.write_manifest()?;
        self.writer
            .finish()
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;
        Ok(())
    }

    /// Returns the full path for a file within the source bundle.
    fn file_path(&self, path: &str) -> String {
        format!("{}/{}", FILES_PATH, path)
    }

    /// Returns a unique path for a file.
    ///
    /// Returns the path if the file does not exist already. Otherwise, a counter is appended to the
    /// file path (e.g. `.1`, `.2`, etc).
    fn unique_path(&self, mut path: String) -> String {
        let mut duplicates = 0;

        while self.manifest.files.contains_key(&path) {
            duplicates += 1;
            match duplicates {
                1 => path.push_str(".1"),
                _ => {
                    use std::fmt::Write;
                    trim_end_matches(&mut path, char::is_numeric);
                    write!(path, ".{}", duplicates).unwrap();
                }
            }
        }

        path
    }

    /// Flushes the manifest file to the bundle.
    fn write_manifest(&mut self) -> Result<(), SourceBundleError> {
        self.writer
            .start_file(MANIFEST_PATH, FileOptions::default())
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;

        serde_json::to_writer(&mut self.writer, &self.manifest)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::BadManifest, e))?;

        Ok(())
    }
}

/// Processes the `source`, looking for `il2cpp` specific reference comments.
///
/// The files referenced by those comments are added to the `referenced_files` Set.
fn collect_il2cpp_sources(source: &[u8], referenced_files: &mut BTreeSet<String>) {
    if let Ok(source) = std::str::from_utf8(source) {
        for line in source.lines() {
            let line = line.trim();

            if let Some(source_ref) = line.strip_prefix("//<source_info:") {
                if let Some((file, _line)) = source_ref.rsplit_once(':') {
                    if !referenced_files.contains(file) {
                        referenced_files.insert(file.to_string());
                    }
                }
            }
        }
    }
}

impl SourceBundleWriter<BufWriter<File>> {
    /// Create a bundle writer that writes its output to the given path.
    ///
    /// If the file does not exist at the given path, it is created. If the file does exist, it is
    /// overwritten.
    pub fn create<P>(path: P) -> Result<SourceBundleWriter<BufWriter<File>>, SourceBundleError>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| SourceBundleError::new(SourceBundleErrorKind::WriteFailed, e))?;

        Self::start(BufWriter::new(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use similar_asserts::assert_eq;
    use tempfile::NamedTempFile;

    #[test]
    fn test_has_file() -> Result<(), SourceBundleError> {
        let writer = Cursor::new(Vec::new());
        let mut bundle = SourceBundleWriter::start(writer)?;

        bundle.add_file("bar.txt", &b"filecontents"[..], SourceFileInfo::default())?;
        assert!(bundle.has_file("bar.txt"));

        bundle.finish()?;
        Ok(())
    }

    #[test]
    fn test_duplicate_files() -> Result<(), SourceBundleError> {
        let writer = Cursor::new(Vec::new());
        let mut bundle = SourceBundleWriter::start(writer)?;

        bundle.add_file("bar.txt", &b"filecontents"[..], SourceFileInfo::default())?;
        bundle.add_file("bar.txt", &b"othercontents"[..], SourceFileInfo::default())?;
        assert!(bundle.has_file("bar.txt"));
        assert!(bundle.has_file("bar.txt.1"));

        bundle.finish()?;
        Ok(())
    }

    #[test]
    fn test_il2cpp_reference() -> Result<(), Box<dyn std::error::Error>> {
        let mut cpp_file = NamedTempFile::new()?;
        let mut cs_file = NamedTempFile::new()?;

        let cpp_contents = format!("foo\n//<source_info:{}:111>\nbar", cs_file.path().display());

        // well, a source bundle itself is an `ObjectLike` :-)
        let object_buf = {
            let mut writer = Cursor::new(Vec::new());
            let mut bundle = SourceBundleWriter::start(&mut writer)?;

            let path = cpp_file.path().to_string_lossy();
            let mut info = SourceFileInfo::new();
            info.set_ty(SourceFileType::Source);
            info.set_path(path.to_string());
            bundle.add_file(path, cpp_contents.as_bytes(), info)?;

            bundle.finish()?;
            writer.into_inner()
        };
        let object = SourceBundle::parse(&object_buf)?;

        // write file contents to temp files
        cpp_file.write_all(cpp_contents.as_bytes())?;
        cs_file.write_all(b"some C# source")?;

        // write the actual source bundle based on the `object`
        let mut output_buf = Cursor::new(Vec::new());
        let mut writer = SourceBundleWriter::start(&mut output_buf)?;
        writer.collect_il2cpp_sources(true);

        let written = writer.write_object(&object, "whatever")?;
        assert!(written);
        let output_buf = output_buf.into_inner();

        // and collect all the included files
        let source_bundle = SourceBundle::parse(&output_buf)?;
        let session = source_bundle.debug_session()?;
        let actual_files: BTreeMap<_, _> = session
            .files()
            .flatten()
            .flat_map(|f| {
                let path = f.abs_path_str();
                session
                    .source_by_path(&path)
                    .ok()
                    .flatten()
                    .map(|c| (path, c.into_owned()))
            })
            .collect();

        let mut expected_files = BTreeMap::new();
        expected_files.insert(cpp_file.path().to_string_lossy().into_owned(), cpp_contents);
        expected_files.insert(
            cs_file.path().to_string_lossy().into_owned(),
            String::from("some C# source"),
        );

        assert_eq!(actual_files, expected_files);

        Ok(())
    }

    #[test]
    fn test_bundle_paths() {
        assert_eq!(sanitize_bundle_path("foo"), "foo");
        assert_eq!(sanitize_bundle_path("foo/bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("/foo/bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("C:/foo/bar"), "C/foo/bar");
        assert_eq!(sanitize_bundle_path("\\foo\\bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("\\\\UNC\\foo\\bar"), "UNC/foo/bar");
    }
}
