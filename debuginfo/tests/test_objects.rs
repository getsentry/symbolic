use failure::Error;

use symbolic_common::{Arch, ByteView};
use symbolic_debuginfo::{Object, ObjectKind, Symbol};

#[test]
fn test_pe() -> Result<(), Error> {
    let view = ByteView::open("../testutils/fixtures/windows/crash.exe")?;
    let file = Object::parse(&view)?;

    assert_eq!(file.id(), "3249d99d-0c40-4931-8610-f4e4fb0b6936-1".parse()?);
    assert_eq!(file.arch(), Arch::X86);
    assert_eq!(file.kind(), ObjectKind::Executable);
    assert_eq!(file.load_address(), 0x0040_0000);

    let symbols = file.symbol_map();
    assert!(symbols.is_empty());

    Ok(())
}

#[test]
fn test_pdb() -> Result<(), Error> {
    let view = ByteView::open("../testutils/fixtures/windows/crash.pdb")?;
    let file = Object::parse(&view)?;

    assert_eq!(file.id(), "3249d99d-0c40-4931-8610-f4e4fb0b6936-1".parse()?);
    assert_eq!(file.arch(), Arch::X86);
    assert_eq!(file.kind(), ObjectKind::Debug);

    let symbols = file.symbol_map();
    assert_eq!(symbols.len(), 139);

    let symbol = symbols.lookup(0x373e);
    assert_eq!(
        symbol,
        Some(&Symbol {
            name: Some("memset".into()),
            address: 0x373e,
            size: 0x6,
        })
    );

    Ok(())
}

#[test]
fn test_macho() {
    // TODO(ja): Implement
}

#[test]
fn test_fat_mach() {
    // TODO(ja): Implement
}

#[test]
fn test_elf() {
    // TODO(ja): Implement
}

#[test]
fn test_breakpad() -> Result<(), Error> {
    let view = ByteView::open("../testutils/fixtures/windows/crash.sym")?;
    let file = Object::parse(&view)?;

    assert_eq!(file.id(), "3249d99d-0c40-4931-8610-f4e4fb0b6936-1".parse()?);
    assert_eq!(file.arch(), Arch::X86);
    assert_eq!(file.kind(), ObjectKind::Debug);
    assert_eq!(file.load_address(), 0);
    assert!(file.has_symbols());

    let symbols = file.symbol_map();
    assert_eq!(symbols.len(), 35);

    assert_eq!(
        symbols[0],
        Symbol {
            name: Some("__CxxFrameHandler3".into()),
            address: 0x3726,
            size: 0x6,
        }
    );

    let symbol = symbols.lookup(0x3753);
    assert_eq!(
        symbol,
        Some(&Symbol {
            name: Some("_callnewh".into()),
            address: 0x3750,
            size: 0x6,
        })
    );

    Ok(())
}
