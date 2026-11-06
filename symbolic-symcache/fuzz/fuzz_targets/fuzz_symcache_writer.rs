#![no_main]

// This is the path Symbolicator takes for uploaded debug files: parse an
// archive, convert each object into a SymCache, and read the result back.
//
// cargo +nightly fuzz run fuzz_symcache_writer -- -max_len=16777216
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(archive) = symbolic_debuginfo::Archive::parse(data) else {
        return;
    };

    for object in archive.objects() {
        let Ok(object) = object else {
            continue;
        };

        let mut converter = symbolic_symcache::SymCacheConverter::new();
        if converter.process_object(&object).is_err() {
            continue;
        }

        let mut buf = Vec::new();
        converter.serialize(&mut buf).unwrap();

        // A cache we just wrote must always be readable.
        let cache = symbolic_symcache::SymCache::parse(&buf).unwrap();
        for function in cache.functions() {
            let _ = function.name();
        }
    }
});
