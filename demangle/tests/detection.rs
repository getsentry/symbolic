//! Language auto-detection tests

extern crate symbolic_common;
extern crate symbolic_demangle;

use symbolic_common::types::{Language, Name};
use symbolic_demangle::Demangle;

fn assert_language(input: &str, lang: Language) {
    let name = Name::new(input);
    assert_eq!(name.detect_language(), Some(lang));
}

fn assert_none(input: &str) {
    let name = Name::new(input);
    assert_eq!(name.detect_language(), None);
}

#[test]
fn unknown() {
    // Fallback to test false positives
    assert_none("xxxxxxxxxxx");
}

#[test]
fn basic_cpp() {
    // This is a symbol generated for "void h(int, char)" by:
    //  - Intel C++ 8.0 for Linux
    //  - HP aC++ A.05.55 IA-64
    //  - IAR EWARM C++ 5.4 ARM
    //  - GCC 3.x and higher
    //  - Clang 1.x and higher[1]
    //
    // NOTE: Microsoft Visual C++ would generate a different
    // symbol, but we do not support that yet.
    assert_language("_Z1hic", Language::Cpp);
}

#[test]
fn basic_rust() {
    assert_language(
        "__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E",
        Language::Rust,
    );
}

#[test]
fn basic_objc_static() {
    assert_language("+[Foo bar:blub:]", Language::ObjC);
}

#[test]
fn basic_objc_member() {
    assert_language("-[Foo bar:blub:]", Language::ObjC);
}

#[test]
fn basic_swift() {
    assert_language("_T08mangling3barSiyKF", Language::Swift);
}

#[test]
fn ambiguous_cpp_rust() {
    // This symbol might look like a Rust symbol at first because of the _ZN...E
    // schema, but comes from a C++ file and is not even a valid Rust.
    // It demangles to:
    //     content::ContentMain(content::ContentMainParams const&)
    assert_language(
        "_ZN7content11ContentMainERKNS_17ContentMainParamsE",
        Language::Cpp,
    );
}
