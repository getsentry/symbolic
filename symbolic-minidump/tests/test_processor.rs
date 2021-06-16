use std::fs::File;
use std::io::{BufRead, BufReader};

use symbolic_common::ByteView;
use symbolic_minidump::cfi::CfiCache;
use symbolic_minidump::processor::{FrameInfoMap, ProcessState};
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
fn process_minidump_linux_cfi() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/mini.dmp"))?;
    let mut frame_info = FrameInfoMap::new();

    let cfi_records = {
        let file = BufReader::new(File::open(fixture("linux/crash.sym"))?);

        file.lines()
            .skip(169) // STACK CFI records start at line 170
            .map(|l| l.unwrap())
            .collect::<Vec<String>>()
            .join("\n")
    };
    let view = ByteView::from_slice(cfi_records.as_bytes());

    frame_info.insert(
        "C0BCC3F19827FE653058404B2831D9E60".parse().unwrap(),
        CfiCache::from_bytes(view).unwrap(),
    );
    let state = ProcessState::from_minidump(&buffer, Some(&frame_info))?;
    insta::assert_debug_snapshot!("process_state_linux_cfi", &state);
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
