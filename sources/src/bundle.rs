//! Defines the `ArtifactBundle` type and corresponding writer.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read};
use std::path::Path;

use failure::ResultExt;
use serde::Serialize;
use zip::{write::FileOptions, ZipWriter};

use crate::error::{ArtifactBundleError, ArtifactBundleErrorKind};

/// Version of the bundle and manifest format.
static BUNDLE_VERSION: u32 = 2;

/// Relative path to the manifest file in the bundle file.
static MANIFEST_PATH: &str = "manifest.json";

/// Path at which files will be written into the bundle.
static FILES_PATH: &str = "files";

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

    /// An optional file path or URL that this file corresponds to.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
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
/// # use symbolic_sources::{ArtifactBundleWriter, ArtifactFileInfo};
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
pub struct ArtifactBundleWriter {
    manifest: ArtifactManifest,
    writer: ZipWriter<BufWriter<File>>,
    finished: bool,
}

impl ArtifactBundleWriter {
    /// Creates a bundle writer on the given file.
    pub fn new(file: File) -> Self {
        ArtifactBundleWriter {
            manifest: ArtifactManifest::new(),
            writer: ZipWriter::new(BufWriter::new(file)),
            finished: false,
        }
    }

    /// Create a bundle writer that writes its output to the given path.
    ///
    /// If the file does not exist at the given path, it is created. If the file does exist, it is
    /// overwritten.
    pub fn create<P>(path: P) -> Result<Self, ArtifactBundleError>
    where
        P: AsRef<Path>,
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .context(ArtifactBundleErrorKind::WriteFailed)?;

        Ok(Self::new(file))
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
        self.manifest.files.contains_key(path.as_ref())
    }

    /// Adds a file and its info to the bundle.
    ///
    /// Multiple files can be added at the same path. For the first duplicate, a counter will be
    /// appended to the file name. Any subsequent duplicate increases that counter. For example:
    ///
    /// ```no_run
    /// # use failure::Error; use std::fs::File;
    /// # use symbolic_sources::{ArtifactBundleWriter, ArtifactFileInfo};
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
        let path = format!("{}/{}", FILES_PATH, path.as_ref());
        let unique_path = self.unique_path(path);

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

    /// Returns a unique path for a file.
    ///
    /// Returns the path if the file does not exist already. Otherwise, a counter is appended to the
    /// file path (e.g. `.1`, `.2`, etc).
    fn unique_path(&self, mut path: String) -> String {
        let mut duplicates = 0;

        while self.has_file(&path) {
            duplicates += 1;
            match duplicates {
                1 => path.push_str(".1"),
                _ => {
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

impl Drop for ArtifactBundleWriter {
    fn drop(&mut self) {
        debug_assert!(self.finished, "ArtifactBundleWriter::finish not called");
    }
}
