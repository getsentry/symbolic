//! A module to bundle sources from debug files for later processing.
//!
//! TODO(jauer): Describe contents
//! Defines the `SourceBundle` type and corresponding writer.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;

use failure::{Fail, ResultExt};
use regex::Regex;
use serde::Serialize;
use zip::{write::FileOptions, ZipWriter};

use crate::{DebugSession, ObjectLike};
use symbolic_common::{clean_path, derive_failure, join_path};

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

/// The type of a file in a bundle.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    /// Regular source file.
    Source,

    /// Minified source code.
    MinifiedSource,

    /// JavaScript sourcemap.
    SourceMap,

    /// Indexed JavaScript RAM bundle.
    IndexedRamBundle,
}

/// Meta data information on a bundled file.
#[derive(Clone, Debug, Default, Serialize)]
pub struct FileInfo {
    /// The type of an bundled file.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<FileType>,

    /// An optional file system path that this file corresponds to.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,

    /// Optional URL that this file corresponds to.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,

    /// Attributes represented as headers.
    ///
    /// This map can include values such as `Content-Type`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

impl FileInfo {
    /// Creates default file information
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if this instance does not carry any information.
    pub fn is_empty(&self) -> bool {
        self.path.is_empty() && self.ty.is_none() && self.headers.is_empty()
    }
}

/// Version number of an source bundle.
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

/// Manifest of an ArtifactBundle containing information on its contents.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SourceBundleManifest {
    /// Descriptors for all artifact files in this bundle.
    #[serde(default)]
    pub files: HashMap<String, FileInfo>,

    /// Arbitrary attributes to include in the bundle.
    #[serde(flatten)]
    pub attributes: BTreeMap<String, String>,
}

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

/// Writer to create source bundles.
///
/// Writers can either [create a new file] or be created from an [existing file]. Then, use
/// [`add_file`] to add files and finally call [`finish`] to flush the archive to
/// the underlying writer.
///
/// Note that dropping the writer
///
/// ```no_run
/// # use failure::Error; use std::fs::File;
/// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, FileInfo};
/// # fn main() -> Result<(), Error> {
/// let mut bundle = SourceBundleWriter::create("bundle.zip")?;
///
/// // Add file called "foo.txt"
/// let file = File::open("my_file.txt")?;
/// bundle.add_file("foo.txt", file, FileInfo::default())?;
///
/// // Flush the bundle to disk
/// bundle.finish()?;
/// # Ok(()) }
/// ```
///
/// [create a new file]: struct.SourceBundleWriter#method.create
/// [existing file]: struct.SourceBundleWriter#method.new
/// [`add_file`]: struct.SourceBundleWriter#method.add_file
/// [`finish`]: struct.SourceBundleWriter#method.finish
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
    /// # use symbolic_debuginfo::sourcebundle::{SourceBundleWriter, FileInfo};
    /// # fn main() -> Result<(), Error> {
    /// let mut bundle = SourceBundleWriter::create("bundle.zip")?;
    ///
    /// // Add file at "foo.txt"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, FileInfo::default())?;
    /// assert!(bundle.has_file("foo.txt"));
    ///
    /// // Add duplicate at "foo.txt.1"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, FileInfo::default())?;
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
        info: FileInfo,
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
    /// It's not permissible to write multiple objects into a bundle.
    pub fn add_object<O>(&mut self, object: &O, object_name: &str) -> Result<(), SourceBundleError>
    where
        O: ObjectLike,
        O::Error: Fail,
    {
        let mut files_handled = BTreeSet::new();
        let mut session = object
            .debug_session()
            .context(SourceBundleErrorKind::BadDebugFile)?;

        self.set_attribute("debug_id", object.debug_id().to_string());
        self.set_attribute("object_name", object_name);

        for func in session.functions() {
            let func = func.context(SourceBundleErrorKind::BadDebugFile)?;
            for line in &func.lines {
                let compilation_dir = String::from_utf8_lossy(&func.compilation_dir);
                let filename = clean_path(&join_path(&compilation_dir, &line.file.path_str()));

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
                    let info = FileInfo {
                        ty: Some(FileType::Source),
                        path: filename.clone(),
                        ..FileInfo::default()
                    };

                    self.add_file(bundle_path, source.as_bytes(), info)
                        .context(SourceBundleErrorKind::WriteFailed)?;
                }

                files_handled.insert(filename);
            }
        }

        Ok(())
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

        bundle.add_file("bar.txt", &b"filecontents"[..], FileInfo::default())?;
        assert!(bundle.has_file("bar.txt"));

        bundle.finish()?;
        Ok(())
    }

    #[test]
    fn test_duplicate_files() -> Result<(), Error> {
        let writer = Cursor::new(Vec::new());
        let mut bundle = SourceBundleWriter::start(writer)?;

        bundle.add_file("bar.txt", &b"filecontents"[..], FileInfo::default())?;
        bundle.add_file("bar.txt", &b"othercontents"[..], FileInfo::default())?;
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
