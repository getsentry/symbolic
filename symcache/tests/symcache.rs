extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_symcache;

use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use symbolic_common::{ByteView, Result};
use symbolic_debuginfo::FatObject;
use symbolic_symcache::SymCache;

fn make_target<P>(target: &str, current_dir: P) -> io::Result<()>
where
    P: AsRef<Path>,
{
    let mut child = Command::new("make")
        .arg(target)
        .current_dir(current_dir)
        .spawn()?;

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "make exited with a non-zero status code",
        ))
    }
}

fn make_fixture(target: &str) -> io::Result<PathBuf> {
    let cwd = env::current_dir()?;
    let fixture_dir = cwd.join("tests/fixtures");

    make_target(target, &fixture_dir)?;
    Ok(fixture_dir.join("build").join(target).join("main"))
}

fn test_symcache(platform: &str) -> Result<()> {
    // Make sure the fixture is built
    let path = make_fixture(platform)?;
    let view = ByteView::from_path(path)?;

    // Open the object file with debug info
    let fat_obj = FatObject::parse(view)?;
    let objects = fat_obj.objects()?;

    // Create the symcache
    let cache = SymCache::from_object(&objects[0])?;
    Ok(())
}

// TODO(ja): upgrade osxcross to include llvm-dsymutil for osx builds
// #[test]
// fn generate_macho_dwarf_symcache() {
//     test_symcache("osx").expect("nope");
// }

#[test]
fn generate_elf_dwarf_symcache() {
    test_symcache("linux").expect("nope");
}
