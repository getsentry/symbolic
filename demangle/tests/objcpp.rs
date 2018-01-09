//! Objective C++ Demangling Tests
//! Objective C++ code can contain both C++ and Objective C symbols. If the
//! language is passed explicitly, the correct demangler must be chosen.

extern crate symbolic_common;
extern crate symbolic_demangle;
mod utils;

use symbolic_common::Language;
use utils::assert_demangle;

#[test]
fn objc() {
    assert_demangle(
        Language::ObjCpp,
        "+[Foo bar:blub:]",
        Some("+[Foo bar:blub:]"),
        Some("+[Foo bar:blub:]"),
    );
}

#[test]
fn cpp() {
    assert_demangle(
        Language::ObjCpp,
        "_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE",
        Some("base::MessagePumpNSApplication::DoRun(base::MessagePump::Delegate*)"),
        Some("base::MessagePumpNSApplication::DoRun"),
    );
}

#[test]
fn cpp_objc_object() {
    assert_demangle(
        Language::ObjCpp,
        "_ZL29SupportsTextureSampleCountMTLPU19objcproto9MTLDevice11objc_objectm",
        Some("SupportsTextureSampleCountMTL(objc_object objcproto9MTLDevice*, unsigned long)"),
        Some("SupportsTextureSampleCountMTL"),
    );
}

#[test]
fn cpp_nsstring() {
    assert_demangle(
        Language::ObjCpp,
        "_ZL19StringContainsEmojiP8NSString",
        Some("StringContainsEmoji(NSString*)"),
        Some("StringContainsEmoji"),
    );
}

#[test]
fn invalid() {
    // If Objective C++ is specified explicitly, the demangler should not fall
    // back to auto-detection. If invalid symbols are passed in, they should not
    // be demangled anymore.
    assert_demangle(Language::ObjCpp, "invalid", None, None);
}
