#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::*;

    #[wasm_bindgen(module = "fs")]
    extern "C" {
        #[wasm_bindgen(js_name = readFileSync)]
        fn read_file_sync(path: &str) -> JsValue;
    }

    pub fn fixture(path: &str) -> Vec<u8> {
        js_sys::Uint8Array::new(&read_file_sync(&fixture_path(path))).to_vec()
    }
}
#[cfg(target_arch = "wasm32")]
pub use self::wasm::*;

#[cfg(not(target_arch = "wasm32"))]
mod non_wasm {
    use super::*;

    pub fn fixture(path: &str) -> Vec<u8> {
        std::fs::read(fixture_path(path)).unwrap()
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub use self::non_wasm::*;

fn fixture_path(path: &str) -> String {
    format!("{dir}/../{path}", dir = env!("CARGO_MANIFEST_DIR"))
}
