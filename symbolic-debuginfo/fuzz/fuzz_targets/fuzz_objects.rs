#![no_main]
use libfuzzer_sys::fuzz_target;

const MH_MAGIC: &[u8] = &0xfeed_face_u32.to_be_bytes();
const MH_MAGIC_64: &[u8] = &0xfeed_facf_u32.to_be_bytes();
const MH_MAGIC_LE: &[u8] = &0xfeed_face_u32.to_le_bytes();
const MH_MAGIC_64_LE: &[u8] = &0xfeed_facf_u32.to_le_bytes();
const FAT_MAGIC: &[u8] = &0xcafe_babe_u32.to_be_bytes();
const FAT_MAGIC_LE: &[u8] = &0xcafe_babe_u32.to_le_bytes();
const WASM_MAGIC: &[u8] = b"\x00asm";
const BREAKPAD_MAGIC: &[u8] = b"MODULE ";
const SOURCEBUNDLE_MAGIC: &[u8] = b"SYSB";
const PDB_MAGIC: &[u8] = b"Microsoft C/C++ MSF 7.00\r\n\x1a\x44\x53\x00\x00\x00";
const PE_MAGIC: &[u8] = &0x5a4d_u16.to_le_bytes();
const ELF_MAGIC: &[u8] = b"\x7FELF";

fuzz_target!(|data: Vec<u8>| {
    // symbolic rejects everything < 16 bytes anyway
    if data.len() < 16 {
        return;
    }
    let mut data = data;

    let magic = match data[0] % 12 {
        // mach-o
        0 => MH_MAGIC,
        1 => MH_MAGIC_64,
        2 => MH_MAGIC_LE,
        3 => MH_MAGIC_64_LE,
        // fat mach-o
        4 => FAT_MAGIC,
        5 => FAT_MAGIC_LE,
        // wasm
        6 => WASM_MAGIC,
        // breakpad
        7 => BREAKPAD_MAGIC,
        // source bundle
        8 => SOURCEBUNDLE_MAGIC,
        // pdb
        9 => PDB_MAGIC,
        // pe
        10 => PE_MAGIC,
        // elf
        _ => ELF_MAGIC,
    };
    let len = magic.len().min(data.len());
    data[..len].copy_from_slice(&magic[..len]);

    if let Ok(arc) = symbolic_debuginfo::Archive::parse(&data) {
        let _ = arc.file_format();
        let num_objects = arc.object_count();

        for idx in 0..num_objects {
            if let Ok(Some(obj)) = arc.object_by_index(idx) {
                test_object(obj);
            }
        }

        // we test both random access, and iteration
        for obj in arc.objects() {
            if let Ok(obj) = obj {
                test_object(obj);
            }
        }
    }
});

fn test_object(obj: symbolic_debuginfo::Object) {
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
        for _ in session.functions() {}
        for _ in session.files() {}
    }
}
