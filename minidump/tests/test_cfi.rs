use std::str;

use failure::Error;
use insta;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_minidump::cfi::{AsciiCfiWriter, CfiCache};

#[test]
fn load_empty_cfi_cache() -> Result<(), Error> {
    let buffer = ByteView::from_slice(&[]);
    let cache = CfiCache::from_bytes(buffer)?;
    assert_eq!(cache.version(), 1);
    Ok(())
}

#[test]
fn cfi_from_elf() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/linux/crash")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_linux`.
    insta::assert_snapshot_matches!("cfi_elf", cfi);

    Ok(())
}

#[test]
fn cfi_from_macho() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/macos/crash")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_macos`.
    insta::assert_snapshot_matches!("cfi_macho", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_linux() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/linux/crash.sym")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot_matches!("cfi_sym_linux", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_macos() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/macos/crash.sym")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot_matches!("cfi_sym_macos", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_windows() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/windows/crash.sym")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot_matches!("cfi_sym_windows", cfi);

    Ok(())
}

#[test]
fn cfi_from_pdb_windows() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/windows/crash.pdb")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot_matches!("cfi_pdb_windows", cfi);

    Ok(())
}

#[test]
fn cfi_from_pe_windows() -> Result<(), Error> {
    let buffer = ByteView::open("../testutils/fixtures/windows/CrashWithException.exe")?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot_matches!("cfi_pe_windows", cfi);

    Ok(())
}
