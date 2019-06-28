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
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;
use std::sync::Arc;

use failure::{Fail, ResultExt};
use lazycell::LazyCell;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use zip::{write::FileOptions, ZipWriter};

use symbolic_common::{derive_failure, Arch, AsSelf, CodeId, DebugId};

use crate::base::*;
use crate::private::Parse;
use crate::{DebugSession, ObjectKind, ObjectLike};

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

/// Variants of [`SourceBundleError`](struct.SourceBundleError.html).
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum SourceBundleErrorKind {
    /// The source bundle container is damanged.
    #[fail(display = "malformed zip archive")]
    BadZip,

    /// The `Object` contains invalid data and cannot be converted.
    #[fail(display = "malformed debug info file")]
    BadDebugFile,

    /// Generic error when writing a source bundle, most likely IO.
    #[fail(display = "failed to write source bundle")]
    WriteFailed,
}

derive_failure!(
    SourceBundleError,
    SourceBundleErrorKind,
    doc = "An error returned when handling `SourceBundles`.",
);

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
    pub files: HashMap<String, SourceFileInfo>,

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
pub struct SourceBundle<'d> {
    manifest: Arc<SourceBundleManifest>,
    archive: Arc<Mutex<zip::read::ZipArchive<std::io::Cursor<&'d [u8]>>>>,
    data: &'d [u8],
}

impl fmt::Debug for SourceBundle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SourceBundle")
            .field("code_id", &self.code_id())
            .field("debug_id", &self.debug_id())
            .field("arch", &self.arch())
            .field("kind", &self.kind())
            .field("load_address", &format_args!("{:#x}", self.load_address()))
            .field("has_symbols", &self.has_symbols())
            .field("has_debug_info", &self.has_debug_info())
            .field("has_unwind_info", &self.has_unwind_info())
            .field("has_source", &self.has_source())
            .finish()
    }
}

impl<'d> SourceBundle<'d> {
    /// Tests whether the buffer could contain a `SourceBundle`.
    pub fn test(bytes: &[u8]) -> bool {
        bytes.starts_with(&BUNDLE_MAGIC)
    }

    /// Tries to parse a `SourceBundle` from the given slice.
    pub fn parse(data: &'d [u8]) -> Result<SourceBundle<'d>, SourceBundleError> {
        let mut archive = zip::read::ZipArchive::new(std::io::Cursor::new(data))
            .context(SourceBundleErrorKind::BadZip)?;
        let manifest_file = archive
            .by_name("manifest.json")
            .context(SourceBundleErrorKind::BadZip)?;
        let manifest =
            serde_json::from_reader(manifest_file).context(SourceBundleErrorKind::BadZip)?;
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
        ObjectKind::Source
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
    pub fn symbols(&self) -> SourceBundleSymbolIterator<'d> {
        SourceBundleSymbolIterator {
            _marker: std::marker::PhantomData,
        }
    }

    /// Returns an ordered map of symbols in the symbol table.
    pub fn symbol_map(&self) -> SymbolMap<'d> {
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
    pub fn debug_session(&self) -> Result<SourceBundleDebugSession<'d>, SourceBundleError> {
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
    pub fn has_source(&self) -> bool {
        true
    }

    /// Returns the raw data of the source bundle.
    pub fn data(&self) -> &'d [u8] {
        self.data
    }

    /// Returns true if this source bundle contains no source code.
    pub fn is_empty(&self) -> bool {
        self.manifest.files.is_empty()
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for SourceBundle<'d> {
    type Ref = SourceBundle<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        unsafe { std::mem::transmute(self) }
    }
}

impl<'d> Parse<'d> for SourceBundle<'d> {
    type Error = SourceBundleError;

    fn parse(data: &'d [u8]) -> Result<Self, Self::Error> {
        SourceBundle::parse(data)
    }

    fn test(data: &'d [u8]) -> bool {
        SourceBundle::test(data)
    }
}

impl<'d> ObjectLike for SourceBundle<'d> {
    type Error = SourceBundleError;
    type Session = SourceBundleDebugSession<'d>;

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

    fn symbol_map(&self) -> SymbolMap<'_> {
        self.symbol_map()
    }

    fn symbols(&self) -> DynIterator<'_, Symbol<'_>> {
        Box::new(self.symbols())
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

    fn has_source(&self) -> bool {
        self.has_source()
    }
}

/// An iterator yielding symbols from a source bundle
///
/// This is always yielding no results.
pub struct SourceBundleSymbolIterator<'d> {
    _marker: std::marker::PhantomData<&'d [u8]>,
}

impl<'d> Iterator for SourceBundleSymbolIterator<'d> {
    type Item = Symbol<'d>;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl std::iter::FusedIterator for SourceBundleSymbolIterator<'_> {}

/// Debug session for SourceBundle objects.
pub struct SourceBundleDebugSession<'d> {
    manifest: Arc<SourceBundleManifest>,
    archive: Arc<Mutex<zip::read::ZipArchive<std::io::Cursor<&'d [u8]>>>>,
    files_by_path: LazyCell<HashMap<String, String>>,
}

impl<'d> SourceBundleDebugSession<'d> {
    /// Returns an iterator over all source files in this debug file.
    pub fn files(&self) -> SourceBundleFileIterator<'_> {
        SourceBundleFileIterator {
            files: self.manifest.files.values(),
        }
    }

    /// Returns an iterator over all functions in this debug file.
    pub fn functions(&self) -> SourceBundleFunctionIterator<'_> {
        SourceBundleFunctionIterator {
            _marker: std::marker::PhantomData,
        }
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

    /// Get the path of a file in this bundle.
    fn zip_path_by_source_path(&self, path: &str) -> Option<&str> {
        self.files_by_path
            .borrow_with(|| self.get_files_by_path())
            .get(path)
            .map(|zip_path| zip_path.as_str())
    }

    fn source_by_zip_path(&self, zip_path: &str) -> Result<Option<String>, SourceBundleError> {
        let mut archive = self.archive.lock();
        let mut file = archive
            .by_name(zip_path)
            .context(SourceBundleErrorKind::BadZip)?;
        let mut source_content = String::new();

        match file.read_to_string(&mut source_content) {
            Ok(_) => Ok(Some(source_content)),
            Err(e) => Err(e).context(SourceBundleErrorKind::BadZip)?,
        }
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

impl<'d> DebugSession for SourceBundleDebugSession<'d> {
    type Error = SourceBundleError;

    fn functions(&self) -> DynIterator<'_, Result<Function<'_>, Self::Error>> {
        Box::new(self.functions())
    }

    fn files(&self) -> DynIterator<'_, Result<FileEntry<'_>, Self::Error>> {
        Box::new(self.files())
    }

    fn source_by_path(&self, path: &str) -> Result<Option<Cow<'_, str>>, Self::Error> {
        self.source_by_path(path)
    }
}

/// An iterator over source files in a SourceBundle object.
pub struct SourceBundleFileIterator<'s> {
    files: std::collections::hash_map::Values<'s, String, SourceFileInfo>,
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
pub struct SourceBundleFunctionIterator<'d> {
    _marker: std::marker::PhantomData<&'d [u8]>,
}

impl<'s> Iterator for SourceBundleFunctionIterator<'s> {
    type Item = Result<Function<'s>, SourceBundleError>;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl std::iter::FusedIterator for SourceBundleFunctionIterator<'_> {}

impl SourceBundleManifest {
    /// Creates a new, empty manifest.
    pub fn new() -> Self {
        Self::default()
    }
}

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
/// # use failure::Error; use std::fs::File;
/// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, SourceFileInfo};
/// # fn main() -> Result<(), Error> {
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
    finished: bool,
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
            .context(SourceBundleErrorKind::WriteFailed)?;

        Ok(SourceBundleWriter {
            manifest: SourceBundleManifest::new(),
            writer: ZipWriter::new(writer),
            finished: false,
        })
    }

    /// Returns whether the bundle contains any files.
    pub fn is_empty(&self) -> bool {
        self.manifest.files.is_empty()
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
    /// # use failure::Error; use std::fs::File;
    /// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, SourceFileInfo};
    /// # fn main() -> Result<(), Error> {
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
            .context(SourceBundleErrorKind::WriteFailed)?;
        std::io::copy(&mut file, &mut self.writer).context(SourceBundleErrorKind::WriteFailed)?;

        self.manifest.files.insert(unique_path, info);
        Ok(())
    }

    /// Writes a single object into the bundle.
    ///
    /// Returns `Ok(true)` if any source files were added to the bundle, or `Ok(false)` if no
    /// sources could be resolved. Otherwise, an error is returned if writing the bundle fails.
    ///
    /// This finishes the source bundle and flushes the underlying writer.
    pub fn write_object<O>(
        mut self,
        object: &O,
        object_name: &str,
    ) -> Result<bool, SourceBundleError>
    where
        O: ObjectLike,
        O::Error: Fail,
    {
        let mut files_handled = BTreeSet::new();
        let session = object
            .debug_session()
            .context(SourceBundleErrorKind::BadDebugFile)?;

        self.set_attribute("arch", object.arch().to_string());
        self.set_attribute("debug_id", object.debug_id().to_string());
        self.set_attribute("object_name", object_name);
        if let Some(code_id) = object.code_id() {
            self.set_attribute("code_id", code_id.to_string());
        }

        for file_result in session.files() {
            let file = file_result.context(SourceBundleErrorKind::BadDebugFile)?;
            let filename = file.abs_path_str();

            if files_handled.contains(&filename) {
                continue;
            }

            let source = if filename.starts_with('<') && filename.ends_with('>') {
                None
            } else {
                fs::read_to_string(&filename).ok()
            };

            if let Some(source) = source {
                let bundle_path = sanitize_bundle_path(&filename);
                let mut info = SourceFileInfo::new();
                info.set_ty(SourceFileType::Source);
                info.set_path(filename.clone());

                self.add_file(bundle_path, source.as_bytes(), info)
                    .context(SourceBundleErrorKind::WriteFailed)?;
            }

            files_handled.insert(filename);
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
            .context(SourceBundleErrorKind::WriteFailed)?;
        self.finished = true;
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
            .context(SourceBundleErrorKind::WriteFailed)?;

        serde_json::to_writer(&mut self.writer, &self.manifest)
            .context(SourceBundleErrorKind::WriteFailed)?;

        Ok(())
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
            .open(path)
            .context(SourceBundleErrorKind::WriteFailed)?;

        Self::start(BufWriter::new(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use failure::Error;

    #[test]
    fn test_has_file() -> Result<(), Error> {
        let writer = Cursor::new(Vec::new());
        let mut bundle = SourceBundleWriter::start(writer)?;

        bundle.add_file("bar.txt", &b"filecontents"[..], SourceFileInfo::default())?;
        assert!(bundle.has_file("bar.txt"));

        bundle.finish()?;
        Ok(())
    }

    #[test]
    fn test_duplicate_files() -> Result<(), Error> {
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
    fn test_bundle_paths() {
        assert_eq!(sanitize_bundle_path("foo"), "foo");
        assert_eq!(sanitize_bundle_path("foo/bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("/foo/bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("C:/foo/bar"), "C/foo/bar");
        assert_eq!(sanitize_bundle_path("\\foo\\bar"), "foo/bar");
        assert_eq!(sanitize_bundle_path("\\\\UNC\\foo\\bar"), "UNC/foo/bar");
    }
}
