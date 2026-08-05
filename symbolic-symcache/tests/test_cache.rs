use std::{fmt, io::Cursor};

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::{
    FunctionsDebug, SymCache, SymCacheConverter, Type, VariableLocation, Variables,
};
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

struct TypeDebug<'data, 'cache>(&'cache SymCache<'data>, Type<'data>);

impl fmt::Debug for TypeDebug<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.1 {
            Type::Primitive(primitive) => f.write_str(primitive.name().unwrap_or("<unknown>")),
            Type::Pointer(pointer) => {
                match pointer.pointee().and_then(|ty| self.0.lookup_type(ty)) {
                    Some(pointee) => write!(f, "{:?}*", TypeDebug(self.0, pointee)),
                    None => f.write_str("void*"),
                }
            }
            _ => f.write_str("<unknown>"),
        }
    }
}

struct VariablesDebug<'data, 'cache>(&'cache SymCache<'data>, Variables<'data, 'cache>);

impl fmt::Debug for VariablesDebug<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for variable in self.1.clone() {
            write!(
                f,
                "{} ({}, ",
                variable.name().unwrap_or("<unknown>"),
                variable.kind()
            )?;
            match variable.ty() {
                Some(ty) => write!(f, "{:?}", TypeDebug(self.0, ty))?,
                None => write!(f, "<unknown>")?,
            }
            write!(f, "): ")?;

            let mut locations = variable.locations().peekable();
            if locations.peek().is_none() {
                writeln!(f, "<unavailable>")?;
                continue;
            }

            for (index, location) in locations.enumerate() {
                if index > 0 {
                    write!(f, ", ")?;
                }

                let end = location.address + location.size;
                match location.location {
                    VariableLocation::Register { id } => {
                        write!(f, "{:#x}..{end:#x} register {id}", location.address)?;
                    }
                    VariableLocation::FrameOffset { offset } => {
                        write!(f, "{:#x}..{end:#x} frame {offset}", location.address)?;
                    }
                }
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

#[test]
fn test_load_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r#"
    SymCache {
        version: 9,
        debug_id: DebugId {
            uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
            appendix: 0,
        },
        arch: Amd64,
        files: 55,
        functions: 697,
        source_locations: 8431,
        ranges: 6975,
        string_bytes: 51929,
        variables: 548,
        variable_locations: 1548,
        types: 12,
    }
    "#);
    Ok(())
}

#[test]
fn test_load_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_linux", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_load_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r#"
    SymCache {
        version: 9,
        debug_id: DebugId {
            uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
            appendix: 0,
        },
        arch: Amd64,
        files: 36,
        functions: 639,
        source_locations: 7382,
        ranges: 5965,
        string_bytes: 42437,
        variables: 419,
        variable_locations: 992,
        types: 8,
    }
    "#);
    Ok(())
}

#[test]
fn test_load_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_macos", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_lookup() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    let source_locations = symcache.lookup(4_458_187_797 - 4_458_131_456);
    let result: Vec<_> = source_locations
        .map(|sl| {
            (
                sl.file().map(|file| file.full_path()).unwrap(),
                sl.line(),
                sl.function(),
            )
        })
        .collect();
    insta::assert_debug_snapshot!("lookup", result);

    Ok(())
}

#[test]
fn test_pdb_srcsrv_remapping() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    let file = cache.lookup(0x1000).next().unwrap().file().unwrap();
    assert_eq!(
        file.full_srcsrv_path().as_deref(),
        Some("depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc")
    );
    assert_eq!(file.srcsrv_revision(), Some("12345"));

    Ok(())
}

#[test]
fn test_lookup_variables_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;

    let source_location = symcache.lookup(0xfe50).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, <unknown>): 0xfe50..0xfe86 register 5
    str (parameter, void*): 0xfe50..0xfe86 register 4
    length (parameter, unsigned int): 0xfe50..0xfe86 register 1
    mdstring (parameter, void*): 0xfe50..0xfe86 register 2
    result (local, bool): <unavailable>
    out (local, <unknown>): 0xfe50..0xff4c frame -80
    out_idx (local, int): <unavailable>
    out_count (local, int): <unavailable>
    "#);

    let source_location = symcache.lookup(0xfe90).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, <unknown>): <unavailable>
    str (parameter, void*): 0xfe86..0xff02 register 6
    length (parameter, unsigned int): 0xfe86..0xff02 register 3
    mdstring (parameter, void*): 0xfe86..0xff02 register 13
    result (local, bool): <unavailable>
    out (local, <unknown>): 0xfe50..0xff4c frame -80
    out_idx (local, int): 0xfe86..0xfed9 register 12
    out_count (local, int): <unavailable>
    "#);

    let source_location = symcache.lookup(0xfecb).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, <unknown>): <unavailable>
    str (parameter, void*): 0xfe86..0xff02 register 6
    length (parameter, unsigned int): 0xfe86..0xff02 register 3
    mdstring (parameter, void*): 0xfe86..0xff02 register 13
    result (local, bool): <unavailable>
    out (local, <unknown>): 0xfe50..0xff4c frame -80
    out_idx (local, int): 0xfe86..0xfed9 register 12
    out_count (local, int): 0xfecb..0xfeee register 15
    "#);

    Ok(())
}

#[test]
fn test_lookup_variables_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;

    let source_location = symcache.lookup(0xea0).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, void*): 0xea0..0xed2 register 5
    str (parameter, void*): 0xea0..0xeb6 register 4
    mdstring (parameter, void*): 0xea0..0xeb1 register 2
    out (local, <unknown>): 0xea0..0xfe7 frame 16
    out_size (local, <unknown>): <unavailable>
    "#);

    let source_location = symcache.lookup(0xeb2).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, void*): 0xea0..0xed2 register 5
    str (parameter, void*): 0xea0..0xeb6 register 4
    mdstring (parameter, void*): 0xeb1..0xf41 register 12
    out (local, <unknown>): 0xea0..0xfe7 frame 16
    out_size (local, <unknown>): <unavailable>
    "#);

    let source_location = symcache.lookup(0xed2).last().unwrap();
    insta::assert_debug_snapshot!(VariablesDebug(&symcache, source_location.variables()), @r#"
    this (parameter, void*): <unavailable>
    str (parameter, void*): 0xeb6..0xf23 register 3
    mdstring (parameter, void*): 0xeb1..0xf41 register 12
    out (local, <unknown>): 0xea0..0xfe7 frame 16
    out_size (local, <unknown>): <unavailable>
    "#);

    Ok(())
}
