//! C++ Itanium Demangling Tests
//! We use cpp_demangle under the hood which runs the libiberty test suite
//! Still, we run some basic regression tests here to detect demangling differences.

#[macro_use]
mod utils;

use symbolic_common::types::Language;

#[test]
fn test_demangle_cpp() {
    assert_demangle!(Language::Cpp, utils::WITH_ARGS, {
        "_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE" => "JS_GetPropertyDescriptorById(JSContext*, JS::Handle<JSObject*>, JS::Handle<jsid>, JS::MutableHandle<JS::PropertyDescriptor>)",
        "_ZN12_GLOBAL__N_15startEv" => "(anonymous namespace)::start()",
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv" => "(anonymous namespace)::hello()::$_0::operator()() const",
        "_Z3MinIiiEDTqultfp_fp0_cl7forwardIT_Efp_Ecl7forwardIT0_Efp0_EEOS0_OS1_" => "decltype (({parm#1}<{parm#2})?((forward<int>)({parm#1})) : ((forward<int>)({parm#2}))) Min<int, int>(int&&, int&&)",

        // Broken in cpp_demangle
        // "_ZN4base8internal13FunctorTraitsIPFvvEvE6InvokeIJEEEvS3_DpOT_" => "void base::internal::FunctorTraits<void (*)(), void>::Invoke<>(void (*)())",
    });
}

#[test]
fn test_demangle_cpp_no_args() {
    assert_demangle!(Language::Cpp, utils::WITHOUT_ARGS, {
        "_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE" => "JS_GetPropertyDescriptorById",
        "_ZN12_GLOBAL__N_15startEv" => "(anonymous namespace)::start",
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv" => "(anonymous namespace)::hello()::$_0::operator() const",

        // Broken in cpp_demangle
        // "_ZN4base8internal13FunctorTraitsIPFvvEvE6InvokeIJEEEvS3_DpOT_" => "void base::internal::FunctorTraits<void (*)(), void>::Invoke<>",
        // "_Z3MinIiiEDTqultfp_fp0_cl7forwardIT_Efp_Ecl7forwardIT0_Efp0_EEOS0_OS1_" => "decltype (({parm#1}<{parm#2})?((forward<int>)({parm#1})) : ((forward<int>)({parm#2}))) Min<int, int>",
    });
}
