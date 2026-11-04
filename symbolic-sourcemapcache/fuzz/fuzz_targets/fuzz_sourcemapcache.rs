#![no_main]

use symbolic_sourcemapcache::{SourceMapCache, SourceMapCacheWriter, SourcePosition};

// The input is a minified source and its sourcemap, separated by a NUL byte.
//
// cargo +nightly fuzz run fuzz_sourcemapcache
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(data) = std::str::from_utf8(data) else {
        return;
    };
    let Some((source, sourcemap)) = data.split_once('\0') else {
        return;
    };

    let Ok(writer) = SourceMapCacheWriter::new(source, sourcemap) else {
        return;
    };
    let mut buf = Vec::new();
    writer.serialize(&mut buf).unwrap();

    // A cache we just wrote must always be readable.
    let cache = SourceMapCache::parse(&buf).unwrap();
    for file in cache.files() {
        let _ = file.name();
        let _ = file.source();
    }

    let lines = source.lines().count() as u32;
    for line in [0, 1, lines / 2, lines, u32::MAX] {
        for column in [0, 1, 80, u32::MAX] {
            if let Some(loc) = cache.lookup(SourcePosition::new(line, column)) {
                let _ = loc.file_name();
                let _ = loc.line_contents();
                let _ = loc.name();
                let _ = loc.scope();
            }
        }
    }
});
