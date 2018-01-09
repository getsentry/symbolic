//! C++ Itanium Demangling Tests
//! We use cpp_demangle under the hood which runs the libiberty test suite
//! Still, we run some tests here -- also to prepare for MSVC.

extern crate symbolic_common;
extern crate symbolic_demangle;
mod utils;

use symbolic_common::Language;
use utils::assert_demangle;

#[test]
fn v8_javascript() {
    assert_demangle(
        Language::Cpp,
        "_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE",
        Some("JS_GetPropertyDescriptorById(JSContext*, JS::Handle<JSObject*>, JS::Handle<jsid>, JS::MutableHandle<JS::PropertyDescriptor>)"),
        Some("JS_GetPropertyDescriptorById"),
    );
}

#[test]
fn anonymous_namespace() {
    assert_demangle(
        Language::Cpp,
        "_ZN12_GLOBAL__N_15startEv",
        Some("(anonymous namespace)::start()"),
        Some("(anonymous namespace)::start"),
    );
}

#[test]
fn lambda() {
    assert_demangle(
        Language::Cpp,
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv",
        Some("(anonymous namespace)::hello()::$_0::operator()() const"),
        Some("(anonymous namespace)::hello::$_0::operator() const"),
    );
}

// TODO: disabled until cpp_demangle fixes this
// #[test]
// fn decltype() {
//     assert_demangle(
//         Language::Cpp,
//         "_Z3MinIiiEDTqultfp_fp0_cl7forwardIT_Efp_Ecl7forwardIT0_Efp0_EEOS0_OS1_",
//         Some("decltype (({parm#1}<{parm#2})?((forward<int>)({parm#1})) : ((forward<int>)({parm#2}))) Min<int, int>(int&&, int&&)"),
//         Some("decltype (({parm#1}<{parm#2})?((forward<int>)({parm#1})) : ((forward<int>)({parm#2}))) Min<int, int>"),
//     );
// }
