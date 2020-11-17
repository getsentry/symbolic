use symbolic_common::ByteView;
use symbolic_minidump::processor::ProcessState;
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

#[test]
fn process_minidump_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("process_state_linux", &state);
    Ok(())
}

#[test]
fn process_minidump_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("process_state_macos", &state);
    Ok(())
}

#[test]
fn process_minidump_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("process_state_windows", &state);
    Ok(())
}

#[test]
fn get_referenced_modules_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("referenced_modules_linux", &state.referenced_modules());
    Ok(())
}

#[test]
fn get_referenced_modules_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("referenced_modules_macos", &state.referenced_modules());
    Ok(())
}

#[test]
fn get_referenced_modules_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/mini.dmp"))?;
    let state = ProcessState::from_minidump(&buffer, None)?;
    insta::assert_debug_snapshot!("referenced_modules_windows", &state.referenced_modules());
    Ok(())
}
