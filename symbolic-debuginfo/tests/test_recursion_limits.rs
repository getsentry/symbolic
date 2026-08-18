use std::assert_matches;
use symbolic_debuginfo::{Object, ParseObjectOptions};

#[test]
fn test_resolve_function() {
    let data = std::fs::read("tests/fixtures/resolve_function_cycle.elf").unwrap();

    let object = Object::parse(&data).unwrap();

    let session = object.debug_session().unwrap();

    let func = session.functions().next().unwrap().unwrap();

    // The recursion error is swallowed, and the missing function name replaced with
    // an empty string.
    assert_eq!(func.name, "");
}

#[test]
fn test_function_inlining() {
    let data = std::fs::read("tests/fixtures/deep_inline.elf").unwrap();
    let mut opts = ParseObjectOptions::default();
    // Use a lower bound that will still work in debug.  The elf file itself has 10K (more?)
    // nested inlinees.
    opts.max_inline_depth = 400;
    let object = Object::parse_with_opts(&data, opts).unwrap();

    let session = object.debug_session().unwrap();

    let func = session.functions().next().unwrap();

    assert_matches!(func, Err(_));
}
