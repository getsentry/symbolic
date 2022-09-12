use std::collections::HashMap;

use symbolic_sourcemapcache::{
    extract_scope_names, NameResolver, ScopeIndex, ScopeLookupResult, SourceContext, SourcePosition,
};
use symbolic_sourcemapcache::{SourceMapCache, SourceMapCacheWriter};
use symbolic_testutils::fixture;

#[test]
fn resolves_scope_names() {
    let src = std::fs::read_to_string(fixture("sourcemapcache/trace/sync.mjs")).unwrap();

    let scopes = extract_scope_names(&src);
    // dbg!(&scopes);
    let scopes: Vec<_> = scopes
        .into_iter()
        .map(|s| (s.0, s.1.map(|n| n.to_string()).filter(|s| !s.is_empty())))
        .collect();
    let index = ScopeIndex::new(scopes).unwrap();

    let ctx = SourceContext::new(&src).unwrap();

    use ScopeLookupResult::*;
    let lookup = |l: u32, c: u32| {
        // NOTE: the browsers use 1-based line/column numbers, while the crates uses
        // 0-based numbers everywhere
        let offset = ctx
            .position_to_offset(SourcePosition::new(l - 1, c - 1))
            .unwrap();
        index.lookup(offset)
    };

    // objectLiteralAnon@http://127.0.0.1:8080/sync.mjs:84:11
    // at Object.objectLiteralAnon (http://127.0.0.1:8080/sync.mjs:84:11)
    assert_eq!(lookup(84, 11), NamedScope("obj.objectLiteralAnon"));

    // objectLiteralMethod@http://127.0.0.1:8080/sync.mjs:81:9
    // at Object.objectLiteralMethod (http://127.0.0.1:8080/sync.mjs:81:9)
    assert_eq!(lookup(81, 9), NamedScope("obj.objectLiteralMethod"));

    // localReassign@http://127.0.0.1:8080/sync.mjs:76:7
    // at localReassign (http://127.0.0.1:8080/sync.mjs:76:7)
    assert_eq!(lookup(76, 7), NamedScope("localReassign"));

    // Klass.prototype.prototypeMethod@http://127.0.0.1:8080/sync.mjs:71:28
    // at Klass.prototypeMethod (http://127.0.0.1:8080/sync.mjs:71:28)
    assert_eq!(
        lookup(71, 28),
        NamedScope("Klass.prototype.prototypeMethod")
    );

    // #privateMethod@http://127.0.0.1:8080/sync.mjs:40:10
    // at Klass.#privateMethod (http://127.0.0.1:8080/sync.mjs:40:10)
    assert_eq!(lookup(40, 10), NamedScope("BaseKlass.#privateMethod"));

    // classCallbackArrow@http://127.0.0.1:8080/sync.mjs:36:24
    // at Klass.classCallbackArrow (http://127.0.0.1:8080/sync.mjs:36:24)
    assert_eq!(lookup(36, 24), NamedScope("BaseKlass.classCallbackArrow"));

    // classCallbackBound/<@http://127.0.0.1:8080/sync.mjs:65:34
    // at http://127.0.0.1:8080/sync.mjs:65:34
    // TODO: should we infer a better name here?
    assert_eq!(lookup(65, 34), AnonymousScope);

    // classCallbackBound@http://127.0.0.1:8080/sync.mjs:65:22
    // at Klass.classCallbackBound (http://127.0.0.1:8080/sync.mjs:65:5)
    assert_eq!(lookup(65, 22), NamedScope("Klass.classCallbackBound"));
    assert_eq!(lookup(65, 5), NamedScope("Klass.classCallbackBound"));

    // classCallbackSelf@http://127.0.0.1:8080/sync.mjs:61:22
    // at Klass.classCallbackSelf (http://127.0.0.1:8080/sync.mjs:61:5)
    assert_eq!(lookup(61, 22), NamedScope("Klass.classCallbackSelf"));
    assert_eq!(lookup(61, 5), NamedScope("Klass.classCallbackSelf"));

    // classMethod/<@http://127.0.0.1:8080/sync.mjs:56:12
    // at http://127.0.0.1:8080/sync.mjs:56:12
    // TODO: should we infer a better name here?
    assert_eq!(lookup(56, 12), AnonymousScope);

    // classMethod@http://127.0.0.1:8080/sync.mjs:55:22
    // at Klass.classMethod (http://127.0.0.1:8080/sync.mjs:55:5)
    assert_eq!(lookup(55, 22), NamedScope("Klass.classMethod"));
    assert_eq!(lookup(55, 5), NamedScope("Klass.classMethod"));

    // BaseKlass@http://127.0.0.1:8080/sync.mjs:32:10
    // at new BaseKlass (http://127.0.0.1:8080/sync.mjs:32:10)
    assert_eq!(lookup(32, 10), NamedScope("new BaseKlass"));

    // Klass@http://127.0.0.1:8080/sync.mjs:50:5
    // at new Klass (http://127.0.0.1:8080/sync.mjs:50:5)
    assert_eq!(lookup(50, 5), NamedScope("new Klass"));

    // staticMethod@http://127.0.0.1:8080/sync.mjs:46:5
    // at Function.staticMethod (http://127.0.0.1:8080/sync.mjs:46:5)
    assert_eq!(lookup(46, 5), NamedScope("Klass.staticMethod"));

    // arrowFn/namedDeclaredCallback/namedImmediateCallback/</<@http://127.0.0.1:8080/sync.mjs:22:17
    // at http://127.0.0.1:8080/sync.mjs:22:17
    // TODO: should we infer a better name here?
    assert_eq!(lookup(22, 17), AnonymousScope);

    // arrowFn/namedDeclaredCallback/namedImmediateCallback/<@http://127.0.0.1:8080/sync.mjs:21:26
    // at http://127.0.0.1:8080/sync.mjs:21:9
    // TODO: should we infer a better name here?
    assert_eq!(lookup(21, 26), AnonymousScope);
    assert_eq!(lookup(21, 9), AnonymousScope);

    // namedImmediateCallback@http://127.0.0.1:8080/sync.mjs:19:24
    // at namedImmediateCallback (http://127.0.0.1:8080/sync.mjs:19:7)
    assert_eq!(lookup(19, 24), NamedScope("namedImmediateCallback"));
    assert_eq!(lookup(19, 7), NamedScope("namedImmediateCallback"));

    // namedDeclaredCallback@http://127.0.0.1:8080/sync.mjs:17:22
    // at namedDeclaredCallback (http://127.0.0.1:8080/sync.mjs:17:5)
    assert_eq!(lookup(17, 22), NamedScope("namedDeclaredCallback"));
    assert_eq!(lookup(17, 5), NamedScope("namedDeclaredCallback"));

    // arrowFn@http://127.0.0.1:8080/sync.mjs:27:20
    // at arrowFn (http://127.0.0.1:8080/sync.mjs:27:3)
    assert_eq!(lookup(27, 20), NamedScope("arrowFn"));
    assert_eq!(lookup(27, 3), NamedScope("arrowFn"));

    // anonFn@http://127.0.0.1:8080/sync.mjs:12:3
    // at anonFn (http://127.0.0.1:8080/sync.mjs:12:3)
    assert_eq!(lookup(12, 3), NamedScope("anonFn"));

    // namedFnExpr@http://127.0.0.1:8080/sync.mjs:8:3
    // at namedFnExpr (http://127.0.0.1:8080/sync.mjs:8:3)
    assert_eq!(lookup(8, 3), NamedScope("namedFnExpr"));

    // namedFn@http://127.0.0.1:8080/sync.mjs:4:3
    // at namedFn (http://127.0.0.1:8080/sync.mjs:4:3)
    assert_eq!(lookup(4, 3), NamedScope("namedFn"));
}

#[test]
fn resolves_token_from_names() {
    let minified = std::fs::read_to_string(fixture("sourcemapcache/preact.module.js")).unwrap();
    let ctx = SourceContext::new(&minified).unwrap();

    let map = std::fs::read_to_string(fixture("sourcemapcache/preact.module.js.map")).unwrap();
    let sm = sourcemap::decode_slice(map.as_bytes()).unwrap();

    let scopes = extract_scope_names(&minified);
    // dbg!(&scopes);

    let resolver = NameResolver::new(&ctx, &sm);

    let resolved_scopes = scopes.into_iter().map(|(range, name)| {
        let minified_name = name.as_ref().map(|n| n.to_string());
        let original_name = name.map(|n| resolver.resolve_name(&n));

        (range, minified_name, original_name)
    });

    for (range, minified, original) in resolved_scopes {
        println!("{range:?}");
        println!("  minified: {minified:?}");
        println!("  original: {original:?}");
    }
}

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

    let sl = cache.lookup(SourcePosition::new(0, 33)).unwrap();
    assert_eq!(sl.file_name(), Some("../src/foo.js"));
    assert_eq!(sl.line(), 1);
    assert_eq!(sl.column(), 8);
    assert_eq!(sl.scope(), ScopeLookupResult::NamedScope("foo"));
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
