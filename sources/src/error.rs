use failure::Fail;

use symbolic_common::derive_failure;

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
