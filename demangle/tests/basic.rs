extern crate symbolic_demangle;
extern crate symbolic_common;

use symbolic_common::{Language, Name};
use symbolic_demangle::{Demangle, DemangleOptions};


fn assert_mangle(input: &str, output: Option<&str>, opts: DemangleOptions) {
    if let Some(rv) = Name::new(input).demangle(opts).unwrap() {
        assert_eq!(Some(rv.as_str()), output);
    } else {
        assert_eq!(None, output);
    }
}


#[test]
fn test_rust_demangle() {
    assert_mangle("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E", Some("std::io::Read::read_to_end"), Default::default());

    let name = Name::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
    assert_eq!(name.detect_language(), Some(Language::Rust));
    assert_eq!(&name.try_demangle(Default::default()), "std::io::Read::read_to_end");

    assert_mangle("__ZN82_$LT$std..sys_common..poison..PoisonError$LT$T$GT$$u20$as$u20$core..fmt..Debug$GT$3fmt17h0853873ca77ac01aE",
                  Some("<std::sys_common::poison::PoisonError<T> as core::fmt::Debug>::fmt"),
                  Default::default());
}

#[test]
fn test_objcpp_cpp_demangle() {
    let name = Name::with_language("_ZN4base24MessagePumpNSApplication5DoRunEPNS_11MessagePump8DelegateE", Language::ObjCpp);
    assert_eq!(&name.try_demangle(Default::default()), "base::MessagePumpNSApplication::DoRun");
}

#[test]
fn test_objcpp_objc_demangle() {
    let name = Name::with_language("+[KSCrashReportFilterObjectForKey filterWithKey:allowNotFound:]", Language::ObjCpp);
    assert_eq!(&name.try_demangle(Default::default()), "+[KSCrashReportFilterObjectForKey filterWithKey:allowNotFound:]");
}

#[test]
fn test_cpp_demangle() {
    assert_mangle("_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE",
                  Some("JS_GetPropertyDescriptorById"), DemangleOptions {
                      with_arguments: false,
                      ..Default::default()
                  });

    let name = Name::new("_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE");
    assert_eq!(name.detect_language(), Some(Language::Cpp));
    assert_eq!(&name.try_demangle(Default::default()), "JS_GetPropertyDescriptorById");
}

#[test]
fn test_cpp_potential_rust_demangle() {
    // TODO: This namebol yields inconsistent results in C++
    let name = Name::new("_ZN4base8internal7InvokerINS0_9BindStateIMN4mate19TrackableObjectBaseEFvvEJNS_7WeakPtrIS4_EEEEEFvvEE7RunImplIRKS6_RKNSt3__15tupleIJS8_EEEJLm0EEEEvOT_OT0_NS_13IndexSequenceIJXspT1_EEEE");
    assert_eq!(name.detect_language(), Some(Language::Cpp));
}

#[test]
fn test_objc_demangle_noop() {
    let name = Name::new("+[KSCrashReportFilterObjectForKey filterWithKey:allowNotFound:]");
    assert_eq!(name.detect_language(), Some(Language::ObjC));
    assert_eq!(&name.try_demangle(Default::default()), "+[KSCrashReportFilterObjectForKey filterWithKey:allowNotFound:]");
}

#[test]
fn test_no_match() {
    assert_mangle("foo", None, Default::default());

    let name = Name::new("bla_bla_bla");
    assert_eq!(name.detect_language(), None);
    assert_eq!(&name.try_demangle(Default::default()), "bla_bla_bla");
}
