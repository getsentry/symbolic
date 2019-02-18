use std::str;

use failure::Error;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_minidump::cfi::{AsciiCfiWriter, CfiCache};
use symbolic_testutils::{assert_snapshot_plain, fixture_path};

#[test]
fn load_empty_cfi_cache() -> Result<(), Error> {
    let buffer = ByteView::from_slice(&[]);
    let cache = CfiCache::from_bytes(buffer)?;
    assert_eq!(cache.version(), 1);
    Ok(())
}

#[test]
fn cfi_from_elf() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_linux.txt`.
    assert_snapshot_plain("cfi_elf.txt", cfi);

    Ok(())
}

#[test]
fn cfi_from_macho() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/crash"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_macos.txt`.
    assert_snapshot_plain("cfi_macho.txt", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    assert_snapshot_plain("cfi_sym_linux.txt", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    assert_snapshot_plain("cfi_sym_macos.txt", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("windows/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    assert_snapshot_plain("cfi_sym_windows.txt", cfi);

    Ok(())
}
