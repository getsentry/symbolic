use std::str;

use symbolic_cfi::{AsciiCfiWriter, CfiCache};
use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture;

use similar_asserts::assert_eq;

type Error = Box<dyn std::error::Error>;

#[test]
fn load_empty_cfi_cache() -> Result<(), Error> {
    let buffer = ByteView::from_slice(&[]);
    let cache = CfiCache::from_bytes(buffer)?;
    assert_eq!(cache.version(), 1);
    Ok(())
}

#[test]
fn cfi_from_elf() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_linux`.
    insta::assert_snapshot!("cfi_elf", cfi);

    Ok(())
}

#[test]
fn cfi_from_macho() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_macos`.
    insta::assert_snapshot!("cfi_macho", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot!("cfi_sym_linux", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot!("cfi_sym_macos", cfi);

    Ok(())
}

#[test]
fn cfi_from_sym_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot!("cfi_sym_windows", cfi);

    Ok(())
}

#[test]
fn cfi_from_pdb_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot!("cfi_pdb_windows", cfi);

    Ok(())
}

#[test]
fn cfi_from_pe_windows() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/CrashWithException.exe"))?;
    let object = Object::parse(&buffer)?;

    let buf: Vec<u8> = AsciiCfiWriter::transform(&object)?;
    let cfi = str::from_utf8(&buf)?;
    insta::assert_snapshot!("cfi_pe_windows", cfi);

    Ok(())
}
