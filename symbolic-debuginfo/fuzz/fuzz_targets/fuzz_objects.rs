#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: Vec<u8>| {
    // symbolic rejects everything < 16 bytes anyway
    if data.len() < 16 {
        return;
    }
    let mut data = data;
    // TODO: make sure we have a valid file magic

    // TODO: maybe use `Archive` instead
    if let Ok(obj) = symbolic_debuginfo::Object::parse(&data) {
        let _ = obj.file_format();
        let _ = obj.code_id();
        let _ = obj.debug_id();
        let _ = obj.arch();
        let _ = obj.kind();
        let _ = obj.load_address();
        let _ = obj.has_symbols();
        let _ = obj.has_debug_info();
        let _ = obj.has_unwind_info();
        let _ = obj.has_sources();
        let _ = obj.is_malformed();

        let _ = obj.symbol_map();
        // the `symbol_map` already exhausts the `symbols` iterator
        // let mut symbols = obj.symbols();

        if let Ok(session) = obj.debug_session() {
            // TODO: iterate over all everything in there...
        }
    }
});
