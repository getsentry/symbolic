//! Test helpers for `symbolic`.
#![warn(missing_docs)]

use std::path::{Path, PathBuf};

/// Returns the full path to the specified fixture.
///
/// Fixtures are stored in the `testutils/fixtures` directory and paths should be given relative to
/// that location.
///
/// # Example
///
/// ```
/// use symbolic_testutils::fixture;
///
/// let path = fixture("macos/crash");
/// assert!(path.ends_with("macos/crash"));
/// ```
pub fn fixture<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    full_path.push("fixtures");

    let path = path.as_ref();
    full_path.push(path);

    assert!(
        full_path.exists(),
        "Fixture does not exist: {}",
        full_path.display()
    );

    full_path
}
