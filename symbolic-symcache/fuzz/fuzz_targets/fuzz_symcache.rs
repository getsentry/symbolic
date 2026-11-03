#![no_main]

// cargo +nightly fuzz run fuzz_symcache
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(cache) = symbolic_symcache::SymCache::parse(data) else {
        return;
    };

    let _ = cache.version();
    let _ = cache.arch();
    let _ = cache.debug_id();

    for function in cache.functions() {
        let _ = function.name();
        let _ = function.entry_pc();
    }
    for file in cache.files() {
        let _ = file.full_path();
    }

    // Probe addresses taken from the input itself, plus the edges.
    let mut addrs = vec![0, u64::MAX];
    addrs.extend(
        data.chunks_exact(8)
            .take(16)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap())),
    );
    for addr in addrs {
        for loc in cache.lookup(addr) {
            let _ = loc.line();
            let _ = loc.file().map(|f| f.full_path());
            let _ = loc.function().name();
        }
    }
});
