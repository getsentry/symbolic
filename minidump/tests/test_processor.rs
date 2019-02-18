use failure::Error;

use symbolic_common::ByteView;
use symbolic_minidump::processor::ProcessState;
use symbolic_testutils::{assert_snapshot, fixture_path};

#[test]
fn process_minidump_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot("process_state_linux.txt", &state);
    Ok(())
}

#[test]
fn process_minidump_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot("process_state_macos.txt", &state);
    Ok(())
}

#[test]
fn process_minidump_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("windows/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot("process_state_windows.txt", &state);
    Ok(())
}

#[test]
fn get_referenced_modules_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot("referenced_modules_linux.txt", &state.referenced_modules());
    Ok(())
}

#[test]
fn get_referenced_modules_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot("referenced_modules_macos.txt", &state.referenced_modules());
    Ok(())
}

#[test]
fn get_referenced_modules_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("windows/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    assert_snapshot(
        "referenced_modules_windows.txt",
        &state.referenced_modules(),
    );
    Ok(())
}
