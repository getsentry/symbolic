#![no_main]

// cargo +nightly fuzz run fuzz_ppdb -j 12 -- -max_len=16777216 # 16M
libfuzzer_sys::fuzz_target!(|data| {
    if let Ok(ppdb) = symbolic_ppdb::PortablePdb::parse(data) {
        let mut writer = symbolic_ppdb::PortablePdbCacheConverter::new();
        if writer.process_portable_pdb(&ppdb).is_ok() {
            let mut buf = Vec::new();
            writer.serialize(&mut buf).unwrap();

            symbolic_ppdb::PortablePdbCache::parse(&buf).unwrap();
        }
    }
});

// This mutator makes sure we always have a valid file magic.
libfuzzer_sys::fuzz_mutator!(
    |data: &mut [u8], size: usize, max_size: usize, _seed: u32| {
        let new_size = libfuzzer_sys::fuzzer_mutate(data, size, max_size);

        let magic = 0x424A_5342u32.to_le_bytes();
        let len = magic.len().min(data.len());
        data[..len].copy_from_slice(&magic[..len]);

        new_size
    }
);
