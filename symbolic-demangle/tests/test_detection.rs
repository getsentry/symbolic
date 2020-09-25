//! Language auto-detection tests

use symbolic_common::{Language, Name};
use symbolic_demangle::Demangle;

fn assert_language(input: &str, lang: Language) {
    let name = Name::new(input);
    assert_eq!(name.detect_language(), lang);
}

#[test]
fn test_unknown() {
    // Fallback to test false positives
    assert_language("xxxxxxxxxxx", Language::Unknown);
}

#[test]
fn test_cpp_gcc() {
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
fn test_cpp_msvc() {
    // The same symbol as above, mangled by MSVC:
    assert_language("?h@@YAXH@Z	", Language::Cpp);
}

#[test]
fn test_objc_static() {
    assert_language("+[Foo bar:blub:]", Language::ObjC);
}

#[test]
fn test_objc_member() {
    assert_language("-[Foo bar:blub:]", Language::ObjC);
}

#[test]
fn test_ambiguous_cpp_rust() {
    // This symbol might look like a legacy Rust symbol at first because of the _ZN...E schema, but
    // comes from a C++ file and is not even a valid Rust.
    // It demangles to:
    //     content::ContentMain(content::ContentMainParams const&)
    assert_language(
        "_ZN7content11ContentMainERKNS_17ContentMainParamsE",
        Language::Cpp,
    );
}

#[cfg(feature = "swift")]
mod swift_tests {
    use super::*;

    #[test]
    fn test_swift_old() {
        assert_language("_T08mangling3barSiyKF", Language::Swift);
    }

    #[test]
    fn test_swift_4() {
        assert_language("$S8mangling6curry1yyF", Language::Swift);
    }

    #[test]
    fn test_swift_5() {
        assert_language("$s8mangling6curry1yyF", Language::Swift);
    }
}

#[cfg(feature = "rust")]
mod rust_tests {
    use super::*;

    #[test]
    fn test_rust_legacy() {
        assert_language(
            "__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E",
            Language::Rust,
        );
    }

    #[test]
    fn test_rust_v0() {
        assert_language("_RNvNtCs1234_7mycrate3foo3bar", Language::Rust);
    }
}
