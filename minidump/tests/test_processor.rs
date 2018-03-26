extern crate symbolic_common;
extern crate symbolic_minidump;
extern crate testutils;

use symbolic_common::ByteView;
use symbolic_minidump::ProcessState;
use testutils::{assert_snapshot, fixture_path};

#[test]
fn process_minidump_linux() {
    let buffer = ByteView::from_path(fixture_path("linux/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot("process_state_linux.txt", &state);
}

#[test]
fn process_minidump_macos() {
    let buffer = ByteView::from_path(fixture_path("macos/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot("process_state_macos.txt", &state);
}

#[test]
fn process_minidump_windows() {
    let buffer = ByteView::from_path(fixture_path("windows/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot("process_state_windows.txt", &state);
}

#[test]
fn get_referenced_modules_linux() {
    let buffer = ByteView::from_path(fixture_path("linux/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot("referenced_modules_linux.txt", &state.referenced_modules());
}

#[test]
fn get_referenced_modules_macos() {
    let buffer = ByteView::from_path(fixture_path("macos/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot("referenced_modules_macos.txt", &state.referenced_modules());
}

#[test]
fn get_referenced_modules_windows() {
    let buffer = ByteView::from_path(fixture_path("windows/mini.dmp"))
        .expect("Could not open the minidump file");
    let state = ProcessState::from_minidump(&buffer, None).expect("Could not process minidump");
    assert_snapshot(
        "referenced_modules_windows.txt",
        &state.referenced_modules(),
    );
}
