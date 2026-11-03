#![no_main]

// cargo +nightly fuzz run fuzz_unreal -- -max_len=16777216
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(crash) = symbolic_unreal::Unreal4Crash::parse_with_limit(data, 16 * 1024 * 1024) else {
        return;
    };

    let _ = crash.name();
    let _ = crash.directory_name();
    for file in crash.files() {
        let _ = file.name();
        let _ = file.ty();
        let _ = file.data().len();
    }
    let _ = crash.native_crash();
    let _ = crash.context();
    let _ = crash.logs(1000);
});
