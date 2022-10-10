use std::collections::HashMap;

use symbolic_sourcemapcache::{
    ScopeLookupResult, SourceMapCache, SourceMapCacheWriter, SourcePosition,
};
use symbolic_testutils::fixture;

#[test]
fn resolves_inlined_function() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/inlining/module.js")).unwrap();
    let map = std::fs::read_to_string(fixture("sourcemapcache/inlining/module.js.map")).unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    let sl = cache.lookup(SourcePosition::new(0, 62)).unwrap();
    assert_eq!(sl.file_name(), Some("../src/app.js"));
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 29);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("buttonCallback"));

    let sl = cache.lookup(SourcePosition::new(0, 46)).unwrap();
    assert_eq!(sl.file_name(), Some("../src/bar.js"));
    assert_eq!(sl.line(), 3);
    assert_eq!(sl.column(), 2);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("bar"));

    // NOTE: The last source position itself does not have a named scope, it truely is an
    // anonymous function. However, the *call* itself has a `name` which we use in its place.
    assert_eq!(sl.name(), Some("foo"));

    let sl = cache.lookup(SourcePosition::new(0, 33)).unwrap();
    assert_eq!(sl.file_name(), Some("../src/foo.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 8);
    assert_eq!(sl.scope(), ScopeLookupResult::AnonymousScope);
}

#[test]
fn writes_simple_cache() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/simple/minified.js")).unwrap();
    let map = std::fs::read_to_string(fixture("sourcemapcache/simple/minified.js.map")).unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    let sl = cache.lookup(SourcePosition::new(0, 10)).unwrap();

    assert_eq!(sl.file_name(), Some("tests/fixtures/simple/original.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 9);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("abcd"));
    assert_eq!(sl.line_contents().unwrap(), "function abcd() {}\n");
}

#[test]
fn resolves_location_from_cache() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/preact.module.js")).unwrap();
    let map = std::fs::read_to_string(fixture("sourcemapcache/preact.module.js.map")).unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    use ScopeLookupResult::*;
    let lookup = |l: u32, c: u32| {
        // NOTE: the browsers use 1-based line/column numbers, while the crates uses
        // 0-based numbers everywhere
        cache.lookup(SourcePosition::new(l - 1, c - 1))
    };

    let sl = lookup(1, 50).unwrap();
    assert_eq!(sl.file_name(), Some("../src/constants.js"));
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 34);
    assert_eq!(sl.scope(), Unknown);

    let sl = lookup(1, 133).unwrap();
    assert_eq!(sl.file_name(), Some("../src/util.js"));
    assert_eq!(sl.line(), 11);
    assert_eq!(sl.column(), 22);
    assert_eq!(sl.scope(), NamedScope("assign"));

    let sl = lookup(1, 482).unwrap();
    assert_eq!(sl.file_name(), Some("../src/create-element.js"));
    assert_eq!(sl.line(), 39);
    assert_eq!(sl.column(), 8);
    assert_eq!(sl.scope(), NamedScope("createElement"));

    let sl = lookup(1, 9780).unwrap();
    assert_eq!(sl.file_name(), Some("../src/component.js"));
    assert_eq!(sl.line(), 181);
    assert_eq!(sl.column(), 4);
    assert_eq!(sl.scope(), Unknown);

    let sl = lookup(1, 9795).unwrap();
    assert_eq!(sl.file_name(), Some("../src/create-context.js"));
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 11);
    assert_eq!(sl.scope(), Unknown);
}

#[test]
fn missing_source_names() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/nofiles.js")).unwrap();
    let map = std::fs::read_to_string(fixture("sourcemapcache/nofiles.js.map")).unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    let files = cache.files().collect::<Vec<_>>();

    assert_eq!(files.len(), 2);

    let sp = SourcePosition::new(0, 38);

    let sl = cache.lookup(sp).unwrap();
    assert_eq!(sl.file().unwrap(), files[0]);
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 8);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("add"));
    assert_eq!(sl.line_contents().unwrap(), "\treturn a + b; // fôo\n")
}

#[test]
fn missing_source_contents() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/preact.module.js")).unwrap();
    let map = std::fs::read_to_string(fixture(
        "sourcemapcache/preact-missing-source-contents.module.js.map",
    ))
    .unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    let mut files: HashMap<_, _> = cache
        .files()
        .map(|file| (file.name().unwrap(), file.source()))
        .collect();

    assert_eq!(files.len(), 12);

    // The source map contains `null` for the sourceContents of `util.js` and `create-element.js` and is missing
    // the sourceContents for `catch-error.js` entirely.
    assert!(files.remove("../src/util.js").unwrap().is_none());
    assert!(files.remove("../src/create-element.js").unwrap().is_none());
    assert!(files
        .remove("../src/diff/catch-error.js")
        .unwrap()
        .is_none());

    // All other source contents should be there.
    for contents in files.values() {
        assert!(contents.is_some());
    }
}

#[test]
fn hermes_scope_lookup() {
    // This SourceMap is generated by the react-native + hermes pipeline.
    // See the rust-sourcemap repo for instructions on how to generate it.
    // Additionally, it was processed by sentry-cli, which basically inlines
    // all the sourceContents.
    let map = std::fs::read_to_string(fixture(
        "sourcemapcache/hermes-metro/react-native-hermes.map",
    ))
    .unwrap();

    let writer = SourceMapCacheWriter::new("", &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    // at foo (address at unknown:1:11940)
    let sl = cache.lookup(SourcePosition::new(0, 11939)).unwrap();
    assert_eq!(sl.file_name(), Some("module.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 10);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("foo"));
    assert_eq!(
        sl.line_contents().unwrap(),
        "    throw new Error(\"lets throw!\");\n"
    );

    // at anonymous (address at unknown:1:11858)
    let sl = cache.lookup(SourcePosition::new(0, 11857)).unwrap();
    assert_eq!(sl.file_name(), Some("input.js"));
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 0);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("<global>"));
    assert_eq!(sl.line_contents().unwrap(), "foo();\n");
}

#[test]
fn metro_scope_lookup() {
    // This is the SourceMap as generated by metro alone,
    // without running the code through hermes.
    let minified =
        std::fs::read_to_string(fixture("sourcemapcache/hermes-metro/react-native-metro.js"))
            .unwrap();
    let map = std::fs::read_to_string(fixture(
        "sourcemapcache/hermes-metro/react-native-metro.js.map",
    ))
    .unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    // e.foo (react-native-metro.js:7:101)
    let sl = cache.lookup(SourcePosition::new(6, 100)).unwrap();
    assert_eq!(sl.file_name(), Some("module.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 10);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("foo"));
    assert_eq!(
        sl.line_contents().unwrap(),
        "    throw new Error(\"lets throw!\");\n"
    );

    // at react-native-metro.js:6:44
    let sl = cache.lookup(SourcePosition::new(5, 43)).unwrap();
    assert_eq!(sl.file_name(), Some("input.js"));
    assert_eq!(sl.line(), 2);
    assert_eq!(sl.column(), 0);
    // NOTE: metro has a special `<global>` scope
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("<global>"));
    assert_eq!(sl.line_contents().unwrap(), "foo();\n");
}

#[test]
fn webpack_scope_lookup() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/webpack/bundle.js")).unwrap();
    let map = std::fs::read_to_string(fixture("sourcemapcache/webpack/bundle.js.map")).unwrap();

    let writer = SourceMapCacheWriter::new(&minified, &map).unwrap();

    let mut buf = vec![];
    writer.serialize(&mut buf).unwrap();

    let cache = SourceMapCache::parse(&buf).unwrap();

    // NOTE: we infer `module.exports` for both frames here, although both functions are named in
    // the original source. The webpack minifier step throws that away for obvious reasons but does
    // not retain a `name` for it.

    // at r.exports (bundle.js:1:85)
    let sl = cache.lookup(SourcePosition::new(0, 84)).unwrap();
    assert_eq!(sl.file_name(), Some("webpack:///./foo.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 8);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("module.exports"));
    assert_eq!(sl.line_contents().unwrap(), "  throw new Error(\"wat\");\n");

    // at r.exports (bundle.js:1:44)
    let sl = cache.lookup(SourcePosition::new(0, 43)).unwrap();
    assert_eq!(sl.file_name(), Some("webpack:///./bar.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 2);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("module.exports"));
    assert_eq!(sl.line_contents().unwrap(), "  f();\n");
}
