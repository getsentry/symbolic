//! Objective C++ Demangling Tests
//! Objective C++ code can contain both C++ and Objective C symbols. If the
//! language is passed explicitly, the correct demangler must be chosen.

extern crate symbolic_common;
extern crate symbolic_demangle;
#[macro_use]
mod utils;

use symbolic_common::types::{Language, Name};
use symbolic_demangle::Demangle;

#[test]
fn test_demangle_objcpp() {
    assert_demangle!(Language::ObjCpp, utils::WITH_ARGS, {
        "+[Foo bar:blub:]" => "+[Foo bar:blub:]",
        "_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE" => "base::MessagePumpNSApplication::DoRun(base::MessagePump::Delegate*)",
        "_ZL29SupportsTextureSampleCountMTLPU19objcproto9MTLDevice11objc_objectm" => "SupportsTextureSampleCountMTL(objc_object objcproto9MTLDevice*, unsigned long)",
        "_ZL19StringContainsEmojiP8NSString" => "StringContainsEmoji(NSString*)",
    });
}

#[test]
fn test_demangle_objcpp_no_args() {
    assert_demangle!(Language::ObjCpp, utils::WITHOUT_ARGS, {
        "+[Foo bar:blub:]" => "+[Foo bar:blub:]",
        "_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE" => "base::MessagePumpNSApplication::DoRun",
        "_ZL29SupportsTextureSampleCountMTLPU19objcproto9MTLDevice11objc_objectm" => "SupportsTextureSampleCountMTL",
        "_ZL19StringContainsEmojiP8NSString" => "StringContainsEmoji",
    });
}

#[test]
fn invalid() {
    // If Objective C++ is specified explicitly, the demangler should not fall
    // back to auto-detection. If invalid symbols are passed in, they should not
    // be demangled anymore.
    let name = Name::with_language("invalid", Language::ObjCpp);
    let result = name.demangle(utils::WITH_ARGS);
    assert_eq!(result, None);
}
