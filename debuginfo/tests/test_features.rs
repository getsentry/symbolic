use failure::Error;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture_path;

#[test]
fn test_features_elf_bin() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash"))?;
    let object = Object::parse(&buffer)?;

    assert!(!object.has_symbols());
    assert!(!object.has_debug_info());
    assert!(object.has_unwind_info());

    Ok(())
}

#[test]
fn test_features_elf_dbg() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    assert!(!object.has_symbols());
    assert!(object.has_debug_info());
    assert!(!object.has_unwind_info());

    Ok(())
}

#[test]
fn test_features_mach_bin() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/crash"))?;
    let object = Object::parse(&buffer)?;

    assert!(object.has_symbols());
    assert!(!object.has_debug_info());
    assert!(object.has_unwind_info());

    Ok(())
}

#[test]
fn test_features_mach_dbg() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    ))?;
    let object = Object::parse(&buffer)?;

    assert!(object.has_symbols());
    assert!(object.has_debug_info());
    assert!(!object.has_unwind_info());

    Ok(())
}

#[test]
fn test_features_breakpad() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("macos/crash.sym"))?;
    let object = Object::parse(&buffer)?;

    assert!(!object.has_symbols());
    assert!(object.has_debug_info());
    assert!(object.has_unwind_info());

    Ok(())
}
