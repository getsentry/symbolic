//! Objective C++ Demangling Tests
//! Objective C++ code can contain both C++ and Objective C symbols. If the
//! language is passed explicitly, the correct demangler must be chosen.

extern crate symbolic_common;
extern crate symbolic_demangle;

use symbolic_demangle::{DemangleFormat, DemangleOptions, Symbol};
use symbolic_common::Language;

const DEMANGLE_FORMAT: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: true,
};

fn assert_demangle(input: &str, output: Option<&str>) {
    let symbol = Symbol::with_language(input, Language::ObjCpp);
    if let Some(rv) = symbol.demangle(&DEMANGLE_FORMAT).unwrap() {
        assert_eq!(Some(rv.as_str()), output);
    } else {
        assert_eq!(None, output);
    }
}

#[test]
fn objc() {
    assert_demangle(
        "+[Foo bar:blub:]",
        Some("+[Foo bar:blub:]"),
    );
}

#[test]
fn cpp() {
    assert_demangle(
        "_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE",
        Some("base::MessagePumpNSApplication::DoRun(base::MessagePump::Delegate*)"),
    );
}

#[test]
fn invalid() {
    // If Objective C++ is specified explicitly, the demangler should not fall
    // back to auto-detection. If invalid symbols are passed in, they should not
    // be demangled anymore.
    assert_demangle("invalid", None);
}
