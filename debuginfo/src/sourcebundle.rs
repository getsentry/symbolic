//! A module to bundle sources from debug files for later processing.
//!
//! TODO(jauer): Describe contents
//! Defines the `ArtifactBundle` type and corresponding writer.

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

/// Version of the bundle and manifest format.
static BUNDLE_VERSION: u32 = 2;

/// Relative path to the manifest file in the bundle file.
static MANIFEST_PATH: &str = "manifest.json";

/// Path at which files will be written into the bundle.
static FILES_PATH: &str = "files";

lazy_static::lazy_static! {
    static ref SANE_PATH_RE: Regex = Regex::new(r#":?[/\\]+"#).unwrap();
}

/// Variants of [`ArtifactBundleError`](struct.ArtifactBundleError.html).
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum ArtifactBundleErrorKind {
    /// The `Object` contains invalid data and cannot be converted.
    #[fail(display = "malformed debug info file")]
    BadDebugFile,

    /// Generic error when writing an artifact bundle, most likely IO.
    #[fail(display = "failed to write artifact bundle")]
    WriteFailed,
}

derive_failure!(
    ArtifactBundleError,
    ArtifactBundleErrorKind,
    doc = "An error returned when handling `ArtifactBundles`.",
);

/// Trims matching suffices of a string in-place.
fn trim_end_matches<F>(string: &mut String, pat: F)
where
    F: FnMut(char) -> bool,
{
    let cutoff = string.trim_end_matches(pat).len();
    string.truncate(cutoff);
}

/// The type of an artifact file.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    /// Regular source file.
    #[serde(rename = "source")]
    Script,

    /// Minified source code.
    #[serde(rename = "minified_source")]
    MinifiedScript,

    /// JavaScript sourcemap.
    SourceMap,

    /// Indexed JavaScript RAM bundle.
    IndexedRamBundle,
}

/// Meta data information on an artifact file.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ArtifactFileInfo {
    /// The type of an artifact file.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub ty: Option<ArtifactType>,

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

impl ArtifactFileInfo {
    /// Creates default artifact information
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if this instance does not carry any information.
    pub fn is_empty(&self) -> bool {
        self.path.is_empty() && self.ty.is_none() && self.headers.is_empty()
    }
}

/// Version number of an artifact bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ArtifaceBundleVersion(pub u32);

impl ArtifaceBundleVersion {
    /// Creates a new artifact bundle version.
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

impl Default for ArtifaceBundleVersion {
    fn default() -> Self {
        Self(BUNDLE_VERSION)
    }
}

/// Manifest of an ArtifactBundle containing information on its contents.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ArtifactManifest {
    /// Version of this artifact bundle.
    ///
    /// The version determines the internal structure and available data.
    pub version: ArtifaceBundleVersion,

    /// Descriptors for all artifact files in this bundle.
    #[serde(default)]
    pub files: HashMap<String, ArtifactFileInfo>,

    /// Arbitrary attributes to include in the bundle.
    #[serde(flatten)]
    pub attributes: BTreeMap<String, String>,
}

impl ArtifactManifest {
    /// Creates a new, empty manifest.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Writer to create artifact bundles.
///
/// Writers can either [create a new file] or be created from an [existing file]. Then, use
/// [`add_file`] to add files and finally call [`finish`] to flush the archive to
/// the underlying writer.
///
/// Note that dropping the writer
///
/// ```no_run
/// # use failure::Error; use std::fs::File;
/// # use symbolic_debuginfo::sourcebundle::{ArtifactBundleWriter, ArtifactFileInfo};
/// # fn main() -> Result<(), Error> {
/// let mut bundle = ArtifactBundleWriter::create("bundle.zip")?;
///
/// // Add file called "foo.txt"
/// let file = File::open("my_file.txt")?;
/// bundle.add_file("foo.txt", file, ArtifactFileInfo::default())?;
///
/// // Flush the bundle to disk
/// bundle.finish()?;
/// # Ok(()) }
/// ```
///
/// [create a new file]: struct.ArtifactBundleWriter#method.create
/// [existing file]: struct.ArtifactBundleWriter#method.new
/// [`add_file`]: struct.ArtifactBundleWriter#method.add_file
/// [`finish`]: struct.ArtifactBundleWriter#method.finish
pub struct ArtifactBundleWriter<W>
where
    W: Seek + Write,
{
    manifest: ArtifactManifest,
    writer: ZipWriter<W>,
    finished: bool,
}

impl<W> ArtifactBundleWriter<W>
where
    W: Seek + Write,
{
    /// Creates a bundle writer on the given file.
    pub fn new(writer: W) -> Self {
        ArtifactBundleWriter {
            manifest: ArtifactManifest::new(),
            writer: ZipWriter::new(writer),
            finished: false,
        }
    }

    /// Sets a meta data attribute of the bundle.
    ///
    /// Attributes are flushed to the bundle when it is [finished]. Thus, they can be retrieved or
    /// changed at any time before flushing the writer.
    ///
    /// If the attribute was set before, the prior value is returned.
    ///
    /// [finished]: struct.ArtifactBundleWriter.html#method.remove_attribute
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
    /// # use symbolic_debuginfo::sourcebundle::{ArtifactBundleWriter, ArtifactFileInfo};
    /// # fn main() -> Result<(), Error> {
    /// let mut bundle = ArtifactBundleWriter::create("bundle.zip")?;
    ///
    /// // Add file at "foo.txt"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, ArtifactFileInfo::default())?;
    /// assert!(bundle.has_file("foo.txt"));
    ///
    /// // Add duplicate at "foo.txt.1"
    /// bundle.add_file("foo.txt", File::open("my_duplicate.txt")?, ArtifactFileInfo::default())?;
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
        info: ArtifactFileInfo,
    ) -> Result<(), ArtifactBundleError>
    where
        S: AsRef<str>,
        R: Read,
    {
        let full_path = self.file_path(path.as_ref());
        let unique_path = self.unique_path(full_path);

        self.writer
            .start_file(unique_path.clone(), FileOptions::default())
            .context(ArtifactBundleErrorKind::WriteFailed)?;
        std::io::copy(&mut file, &mut self.writer).context(ArtifactBundleErrorKind::WriteFailed)?;

        self.manifest.files.insert(unique_path, info);
        Ok(())
    }

    /// Writes the manifest to the bundle and flushes the underlying file handle.
    pub fn finish(mut self) -> Result<(), ArtifactBundleError> {
        self.write_manifest()?;
        self.writer
            .finish()
            .context(ArtifactBundleErrorKind::WriteFailed)?;
        self.finished = true;
        Ok(())
    }

    /// Returns the full path for a file within the artifact bundle.
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
    fn write_manifest(&mut self) -> Result<(), ArtifactBundleError> {
        self.writer
            .start_file(MANIFEST_PATH, FileOptions::default())
            .context(ArtifactBundleErrorKind::WriteFailed)?;

        serde_json::to_writer(&mut self.writer, &self.manifest)
            .context(ArtifactBundleErrorKind::WriteFailed)?;

        Ok(())
    }
}

impl ArtifactBundleWriter<BufWriter<File>> {
    /// Create a bundle writer that writes its output to the given path.
    ///
    /// If the file does not exist at the given path, it is created. If the file does exist, it is
    /// overwritten.
    pub fn create<P>(path: P) -> Result<ArtifactBundleWriter<BufWriter<File>>, ArtifactBundleError>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .context(ArtifactBundleErrorKind::WriteFailed)?;

        Ok(Self::new(BufWriter::new(file)))
    }
}

fn sanitize_bundle_path(path: &str) -> String {
    let mut sanitized = SANE_PATH_RE.replace_all(path, "/").into_owned();
    if sanitized.starts_with('/') {
        sanitized.remove(0);
    }
    sanitized
}

/// Writes sources of `Object` files to an artifact bundle.
pub struct DebugSourceWriter<W>
where
    W: Seek + Write,
{
    bundle: ArtifactBundleWriter<W>,
    files_handled: BTreeSet<String>,
}

impl<W> DebugSourceWriter<W>
where
    W: Write + Seek,
{
    /// Creates a new source writer around an artifact bundle writer.
    pub fn new(bundle: ArtifactBundleWriter<W>) -> Self {
        DebugSourceWriter {
            bundle,
            files_handled: BTreeSet::new(),
        }
    }

    /// Writes all source files referenced by functions in this object file to the bundle.
    pub fn write_object<O>(
        &mut self,
        object: &O,
        object_name: &str,
    ) -> Result<(), ArtifactBundleError>
    where
        O: ObjectLike,
        O::Error: Fail,
    {
        let mut session = object
            .debug_session()
            .context(ArtifactBundleErrorKind::BadDebugFile)?;

        self.bundle
            .set_attribute("debug_id", object.debug_id().to_string());
        self.bundle.set_attribute("object_name", object_name);

        for func in session.functions() {
            let func = func.context(ArtifactBundleErrorKind::BadDebugFile)?;
            for line in &func.lines {
                let compilation_dir = String::from_utf8_lossy(&func.compilation_dir);
                let filename = clean_path(&join_path(&compilation_dir, &line.file.path_str()));

                if self.files_handled.contains(&filename) {
                    continue;
                }

                let source = if filename.starts_with('<') && filename.ends_with('>') {
                    None
                } else {
                    fs::read_to_string(&filename).ok()
                };

                if let Some(source) = source {
                    let bundle_path = sanitize_bundle_path(&filename);
                    let info = ArtifactFileInfo {
                        ty: Some(ArtifactType::Script),
                        path: filename.clone(),
                        ..ArtifactFileInfo::default()
                    };

                    self.bundle
                        .add_file(bundle_path, source.as_bytes(), info)
                        .context(ArtifactBundleErrorKind::WriteFailed)?;
                }

                self.files_handled.insert(filename);
            }
        }

        Ok(())
    }

    /// Finishes writing the object file and returns the bundle writer.
    pub fn finish(self) -> Result<ArtifactBundleWriter<W>, ArtifactBundleError> {
        Ok(self.bundle)
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
        let mut bundle = ArtifactBundleWriter::new(writer);

        bundle.add_file("bar.txt", &b"filecontents"[..], ArtifactFileInfo::default())?;
        assert!(bundle.has_file("bar.txt"));

        bundle.finish()?;
        Ok(())
    }

    #[test]
    fn test_duplicate_files() -> Result<(), Error> {
        let writer = Cursor::new(Vec::new());
        let mut bundle = ArtifactBundleWriter::new(writer);

        bundle.add_file("bar.txt", &b"filecontents"[..], ArtifactFileInfo::default())?;
        bundle.add_file(
            "bar.txt",
            &b"othercontents"[..],
            ArtifactFileInfo::default(),
        )?;
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
