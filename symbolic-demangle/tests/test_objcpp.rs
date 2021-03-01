//! Objective C++ Demangling Tests
//! Objective C++ code can contain both C++ and Objective C symbols. If the
//! language is passed explicitly, the correct demangler must be chosen.

#[macro_use]
mod utils;

use symbolic_common::{Language, Name, NameMangling};
use symbolic_demangle::{Demangle, DemangleOptions};

use similar_asserts::assert_eq;

#[test]
#[cfg(feature = "cpp")]
fn test_demangle_objcpp() {
    assert_demangle!(Language::ObjCpp, DemangleOptions::name_only().parameters(true), {
        "+[Foo bar:blub:]" => "+[Foo bar:blub:]",
        "_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE" => "base::MessagePumpNSApplication::DoRun(base::MessagePump::Delegate*)",
        "_ZL29SupportsTextureSampleCountMTLPU19objcproto9MTLDevice11objc_objectm" => "SupportsTextureSampleCountMTL(objc_object objcproto9MTLDevice*, unsigned long)",
        "_ZL19StringContainsEmojiP8NSString" => "StringContainsEmoji(NSString*)",
    });
}

#[test]
#[cfg(feature = "cpp")]
fn test_demangle_objcpp_no_args() {
    assert_demangle!(Language::ObjCpp, DemangleOptions::name_only(), {
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
    let name = Name::new("invalid", NameMangling::Unknown, Language::ObjCpp);
    let result = name.demangle(DemangleOptions::complete());
    assert_eq!(result, None);
}
