extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_minidump;
extern crate testutils;

use std::str;
use symbolic_common::ByteView;
use symbolic_debuginfo::FatObject;
use symbolic_minidump::BreakpadAsciiCfiWriter;
use testutils::{assert_snapshot_plain, fixture_path};

#[test]
fn cfi_from_elf() {
    let buffer = ByteView::from_path(fixture_path("linux/crash"))
        .expect("Could not open the executable file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat.get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let mut cfi = Vec::new();
    {
        let mut writer = BreakpadAsciiCfiWriter::new(&mut cfi);
        writer.process(&object).expect("Could not write CFI");
    }

    let cfi = str::from_utf8(&cfi).expect("Invalid CFI encoding");
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_linux.txt`.
    assert_snapshot_plain("cfi_elf.txt", cfi);
}

#[test]
fn cfi_from_macho() {
    let buffer =
        ByteView::from_path(fixture_path("macos/crash")).expect("Could not open the symbol file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat.get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let mut cfi = Vec::new();
    {
        let mut writer = BreakpadAsciiCfiWriter::new(&mut cfi);
        writer.process(&object).expect("Could not write CFI");
    }

    let cfi = str::from_utf8(&cfi).expect("Invalid CFI encoding");
    // NOTE: Breakpad's CFI writer outputs registers in alphabetical order. We
    // write the CFA register first, and then order by register number. Thus,
    // the output is not identical to `cfi_sym_macos.txt`.
    assert_snapshot_plain("cfi_macho.txt", cfi);
}

#[test]
fn cfi_from_sym_linux() {
    let buffer = ByteView::from_path(fixture_path("linux/crash.sym"))
        .expect("Could not open the symbol file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat.get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let mut cfi = Vec::new();
    {
        let mut writer = BreakpadAsciiCfiWriter::new(&mut cfi);
        writer.process(&object).expect("Could not write CFI");
    }

    let cfi = str::from_utf8(&cfi).expect("Invalid CFI encoding");
    assert_snapshot_plain("cfi_sym_linux.txt", cfi);
}

#[test]
fn cfi_from_sym_macos() {
    let buffer = ByteView::from_path(fixture_path("macos/crash.sym"))
        .expect("Could not open the symbol file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat.get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let mut cfi = Vec::new();
    {
        let mut writer = BreakpadAsciiCfiWriter::new(&mut cfi);
        writer.process(&object).expect("Could not write CFI");
    }

    let cfi = str::from_utf8(&cfi).expect("Invalid CFI encoding");
    assert_snapshot_plain("cfi_sym_macos.txt", cfi);
}

#[test]
fn cfi_from_sym_windows() {
    let buffer = ByteView::from_path(fixture_path("windows/crash.sym"))
        .expect("Could not open the symbol file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat.get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let mut cfi = Vec::new();
    {
        let mut writer = BreakpadAsciiCfiWriter::new(&mut cfi);
        writer.process(&object).expect("Could not write CFI");
    }

    let cfi = str::from_utf8(&cfi).expect("Invalid CFI encoding");
    assert_snapshot_plain("cfi_sym_windows.txt", cfi);
}
