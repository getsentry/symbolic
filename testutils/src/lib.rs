extern crate difference;

use difference::Changeset;
use std::{fmt, io};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::prelude::*;

/// Loads the file at the given location and returns its contents as string.
fn load_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Resolves the full path to a fixture file.
pub fn fixture_path<S: AsRef<str>>(file_name: S) -> PathBuf {
    Path::new("..")
        .join("testutils")
        .join("fixtures")
        .join(file_name.as_ref())
}

/// Loads the fixture file with the given name and returns its contents
/// as String.
pub fn load_fixture<S: AsRef<str>>(file_name: S) -> io::Result<String> {
    load_file(fixture_path(file_name))
}

/// Asserts that the given object matches the snapshot saved in the snapshot
/// file. The object is serialized using the Debug trait.
///
/// If the value differs from the snapshot, the assertion fails and prints
/// a colored diff output.
pub fn assert_snapshot<S: AsRef<str>, T: fmt::Debug>(snapshot_name: S, val: &T) {
    assert_snapshot_plain(snapshot_name, &format!("{:#?}", val));
}

/// Asserts that the given string matches the snapshot saved in the snapshot
/// file. The given string will be used as plain output and directly compared
/// with the stored snapshot.
///
/// If the value differs from the snapshot, the assertion fails and prints
/// a colored diff output. One trailing newline in the snapshot output is
/// ignored by default.
pub fn assert_snapshot_plain<S: AsRef<str>>(snapshot_name: S, output: &str) {
    let name = snapshot_name.as_ref();

    let snapshot_path = Path::new("tests").join("snapshots").join(name);
    let snapshot = load_file(snapshot_path).unwrap_or("".into());

    let expected = if snapshot.ends_with("\n") && !output.ends_with("\n") {
        &snapshot[0..snapshot.len() - 1]
    } else {
        &snapshot
    };

    assert!(
        expected == output,
        "Value does not match stored snapshot {}:\n\n{}",
        name,
        Changeset::new(expected, &output, "\n")
    );
}
