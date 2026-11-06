#![no_main]

use symbolic_common::{Language, Name, NameMangling};
use symbolic_demangle::{Demangle, DemangleOptions};

// Exercises every demangler, including the C++ Swift demangler compiled in
// through `cc`, which OSS-Fuzz builds with the sanitizer flags.
//
// cargo +nightly fuzz run fuzz_demangle
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Ok(ident) = std::str::from_utf8(data) else {
        return;
    };

    let _ = symbolic_demangle::demangle(ident);

    for lang in [
        Language::Unknown,
        Language::Cpp,
        Language::ObjC,
        Language::ObjCpp,
        Language::Rust,
        Language::Swift,
    ] {
        let name = Name::new(ident, NameMangling::Mangled, lang);
        let _ = name.demangle(DemangleOptions::complete());
        let _ = name.demangle(DemangleOptions::name_only());
    }
});
