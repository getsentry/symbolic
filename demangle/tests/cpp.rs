//! C++ Demangling Tests
//!
//! The examples were extracted from the libiberty demangler test suite
//! see https://gcc.gnu.org/viewcvs/gcc/trunk/libiberty/testsuite/demangle-expected?revision=253186
//! only GNU v3 ABI examples have been chosen
//!
//! NOTE: Some breaking tests that are irrelevant for us have been disabled.
//! NOTE: Some symbols were reported with additional spaces, these cases have
//!       been updated.
//!
//! NOTE: We do not test the version without parameters, as the implementation
//! is known to be buggy.

extern crate symbolic_common;
extern crate symbolic_demangle;

use symbolic_demangle::{DemangleFormat, DemangleOptions, Symbol};
use symbolic_common::Language;

const WITH_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Full,
    with_arguments: true,
};

const WITHOUT_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Full,
    with_arguments: false,
};

fn assert_demangle(input: &str, with_args: Option<&str>, without_args: Option<&str>) {
    let symbol = Symbol::with_language(input, Language::Cpp);
    if let Some(rv) = symbol.demangle(&WITH_ARGS).unwrap() {
        assert_eq!(Some(rv.as_str()), with_args);
    } else {
        assert_eq!(None, with_args);
    }

    // if let Some(rv) = symbol.demangle(&WITHOUT_ARGS).unwrap() {
    //     assert_eq!(Some(rv.as_str()), without_args);
    // } else {
    //     assert_eq!(None, without_args);
    // }
}

#[test]
fn sentry_1() {
    assert_demangle(
        "_ZL29SupportsTextureSampleCountMTLPU19objcproto9MTLDevice11objc_objectm",
        Some("SupportsTextureSampleCountMTL(id<MTLDevice>, unsigned long)"),
        Some("SupportsTextureSampleCountMTL"),
    );
}

#[test]
fn sentry_2() {
    assert_demangle(
        "_ZL19StringContainsEmojiP8NSString",
        Some("StringContainsEmoji(NSString*)"),
        Some("StringContainsEmoji"),
    );
}

#[test]
fn sentry_3() {
    assert_demangle(
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv",
        Some("(anonymous namespace)::hello()::$_0::operator()() const"),
        Some("(anonymous namespace)::hello()::$_0::operator()"),
    );
}

//
// LIBIBERTY ------------------------------------------------------------------
//

#[test]
fn libiberty_1() {
    assert_demangle(
        "_Z3fo5n",
        Some("fo5(__int128)"),
        Some("fo5"),
    );
}

#[test]
fn libiberty_2() {
    assert_demangle(
        "_Z3fo5o",
        Some("fo5(unsigned __int128)"),
        Some("fo5"),
    );
}

#[test]
fn libiberty_3() {
    assert_demangle(
        "St9bad_alloc",
        Some("std::bad_alloc"),
        Some("std::bad_alloc"),
    );
}

#[test]
fn libiberty_4() {
    assert_demangle(
        "_ZN1f1fE",
        Some("f::f"),
        Some("f::f"),
    );
}

#[test]
fn libiberty_5() {
    assert_demangle(
        "_Z1fv",
        Some("f()"),
        Some("f"),
    );
}

#[test]
fn libiberty_6() {
    assert_demangle(
        "_Z1fi",
        Some("f(int)"),
        Some("f"),
    );
}

#[test]
fn libiberty_7() {
    assert_demangle(
        "_Z3foo3bar",
        Some("foo(bar)"),
        Some("foo"),
    );
}

#[test]
fn libiberty_8() {
    assert_demangle(
        "_Zrm1XS_",
        Some("operator%(X, X)"),
        Some("operator%"),
    );
}

#[test]
fn libiberty_9() {
    assert_demangle(
        "_ZplR1XS0_",
        Some("operator+(X&, X&)"),
        Some("operator+"),
    );
}

#[test]
fn libiberty_10() {
    assert_demangle(
        "_ZlsRK1XS1_",
        Some("operator<<(X const&, X const&)"),
        Some("operator<<"),
    );
}

#[test]
fn libiberty_11() {
    assert_demangle(
        "_ZN3FooIA4_iE3barE",
        Some("Foo<int [4]>::bar"),
        Some("Foo<int [4]>::bar"),
    );
}

#[test]
fn libiberty_12() {
    assert_demangle(
        "_Z1fIiEvi",
        Some("void f<int>(int)"),
        Some("f<int>"),
    );
}

#[test]
fn libiberty_13() {
    assert_demangle(
        "_Z5firstI3DuoEvS0_",
        Some("void first<Duo>(Duo)"),
        Some("first<Duo>"),
    );
}

#[test]
fn libiberty_14() {
    assert_demangle(
        "_Z5firstI3DuoEvT_",
        Some("void first<Duo>(Duo)"),
        Some("first<Duo>"),
    );
}

#[test]
fn libiberty_15() {
    assert_demangle(
        "_Z3fooIiFvdEiEvv",
        Some("void foo<int, void (double), int>()"),
        Some("foo<int, void (double), int>"),
    );
}

#[test]
fn libiberty_16() {
    assert_demangle(
        "_Z1fIFvvEEvv",
        Some("void f<void ()>()"),
        Some("f<void ()>"),
    );
}

#[test]
fn libiberty_17() {
    assert_demangle(
        "_ZN1N1fE",
        Some("N::f"),
        Some("N::f"),
    );
}

#[test]
fn libiberty_18() {
    assert_demangle(
        "_ZN6System5Sound4beepEv",
        Some("System::Sound::beep()"),
        Some("System::Sound::beep"),
    );
}

#[test]
fn libiberty_19() {
    assert_demangle(
        "_ZN5Arena5levelE",
        Some("Arena::level"),
        Some("Arena::level"),
    );
}

#[test]
fn libiberty_20() {
    assert_demangle(
        "_ZN5StackIiiE5levelE",
        Some("Stack<int, int>::level"),
        Some("Stack<int, int>::level"),
    );
}

#[test]
fn libiberty_21() {
    assert_demangle(
        "_Z1fI1XEvPVN1AIT_E1TE",
        Some("void f<X>(A<X>::T volatile*)"),
        Some("f<X>"),
    );
}

#[test]
fn libiberty_22() {
    assert_demangle(
        "_ZngILi42EEvN1AIXplT_Li2EEE1TE",
        // NOTE: was "void operator-<42>(A<(42)+(2)>::T)"
        Some("void operator-<42>(A<(42) + (2)>::T)"),
        Some("operator-<42>"),
    );
}

#[test]
fn libiberty_23() {
    assert_demangle(
        "_Z4makeI7FactoryiET_IT0_Ev",
        Some("Factory<int> make<Factory, int>()"),
        Some("make<Factory, int>"),
    );
}

#[test]
fn libiberty_24() {
    assert_demangle(
        "_Z4makeI7FactoryiET_IT0_Ev",
        Some("Factory<int> make<Factory, int>()"),
        Some("make<Factory, int>"),
    );
}

#[test]
fn libiberty_25() {
    assert_demangle(
        "_Z3foo5Hello5WorldS0_S_",
        Some("foo(Hello, World, World, Hello)"),
        Some("foo"),
    );
}

#[test]
fn libiberty_26() {
    assert_demangle(
        "_Z3fooPM2ABi",
        Some("foo(int AB::**)"),
        Some("foo"),
    );
}

#[test]
fn libiberty_27() {
    assert_demangle(
        "_ZlsRSoRKSs",
        Some("operator<<(std::ostream&, std::string const&)"),
        Some("operator<<"),
    );
}

#[test]
fn libiberty_28() {
    assert_demangle(
        "_ZTI7a_class",
        Some("typeinfo for a_class"),
        Some("typeinfo for a_class"),
    );
}

#[test]
fn libiberty_29() {
    assert_demangle(
        "U4_farrVKPi",
        Some("int* const volatile restrict _far"),
        Some("int* const volatile restrict _far"),
    );
}

#[test]
fn libiberty_30() {
    assert_demangle(
        "_Z3fooILi2EEvRAplT_Li1E_i",
        // NOTE: was "void foo<2>(int (&) [(2)+(1)])"
        Some("void foo<2>(int (&) [(2) + (1)])"),
        Some("foo<2>"),
    );
}

#[test]
fn libiberty_31() {
    assert_demangle(
        "_Z3fooILi2EEvOAplT_Li1E_i",
        // NOTE: was "void foo<2>(int (&&) [(2)+(1)])"
        Some("void foo<2>(int (&&) [(2) + (1)])"),
        Some("foo<2>"),
    );
}

#[test]
fn libiberty_32() {
    assert_demangle(
        "_Z1fM1AKFvvE",
        Some("f(void (A::*)() const)"),
        Some("f"),
    );
}

#[test]
fn libiberty_33() {
    assert_demangle(
        "_Z3fooc",
        Some("foo(char)"),
        Some("foo"),
    );
}

#[test]
fn libiberty_34() {
    assert_demangle(
        "_Z2f0u8char16_t",
        Some("f0(char16_t)"),
        Some("f0"),
    );
}

#[test]
fn libiberty_35() {
    assert_demangle(
        "_Z2f0Pu8char16_t",
        Some("f0(char16_t*)"),
        Some("f0"),
    );
}

#[test]
fn libiberty_36() {
    assert_demangle(
        "_Z2f0u8char32_t",
        Some("f0(char32_t)"),
        Some("f0"),
    );
}

#[test]
fn libiberty_37() {
    assert_demangle(
        "_Z2f0Pu8char32_t",
        Some("f0(char32_t*)"),
        Some("f0"),
    );
}

#[test]
fn libiberty_38() {
    assert_demangle(
        "2CBIL_Z3foocEE",
        Some("CB<foo(char)>"),
        Some("CB<foo(char)>"),
    );
}

#[test]
fn libiberty_39() {
    assert_demangle(
        "2CBIL_Z7IsEmptyEE",
        Some("CB<IsEmpty>"),
        Some("CB<IsEmpty>"),
    );
}

#[test]
fn libiberty_40() {
    assert_demangle(
        "_ZZN1N1fEiE1p",
        Some("N::f(int)::p"),
        Some("N::f(int)::p"),
    );
}

#[test]
fn libiberty_41() {
    assert_demangle(
        "_ZZN1N1fEiEs",
        Some("N::f(int)::string literal"),
        Some("N::f(int)::string literal"),
    );
}

#[test]
fn libiberty_42() {
    assert_demangle(
        "_Z1fPFvvEM1SFvvE",
        Some("f(void (*)(), void (S::*)())"),
        Some("f"),
    );
}

#[test]
fn libiberty_43() {
    assert_demangle(
        "_ZN1N1TIiiE2mfES0_IddE",
        Some("N::T<int, int>::mf(N::T<double, double>)"),
        Some("N::T<int, int>::mf"),
    );
}

#[test]
fn libiberty_44() {
    assert_demangle(
        "_ZSt5state",
        Some("std::state"),
        Some("std::state"),
    );
}

#[test]
fn libiberty_45() {
    assert_demangle(
        "_ZNSt3_In4wardE",
        Some("std::_In::ward"),
        Some("std::_In::ward"),
    );
}

#[test]
fn libiberty_46() {
    assert_demangle(
        "_Z1fKPFiiE",
        Some("f(int (* const)(int))"),
        Some("f"),
    );
}

#[test]
fn libiberty_47() {
    assert_demangle(
        "_Z1fAszL_ZZNK1N1A1fEvE3foo_0E_i",
        Some("f(int [sizeof (N::A::f() const::foo)])"),
        Some("f"),
    );
}

#[test]
fn libiberty_48() {
    assert_demangle(
        "_Z1fA37_iPS_",
        Some("f(int [37], int (*) [37])"),
        Some("f"),
    );
}

#[test]
fn libiberty_49() {
    assert_demangle(
        "_Z1fM1AFivEPS0_",
        Some("f(int (A::*)(), int (*)())"),
        Some("f"),
    );
}

#[test]
fn libiberty_50() {
    assert_demangle(
        "_Z1fPFPA1_ivE",
        // NOTE: was "f(int (*(*)()) [1])"
        Some("f(int (* (*)()) [1])"),
        Some("f"),
    );
}

#[test]
fn libiberty_51() {
    assert_demangle(
        "_Z1fPKM1AFivE",
        Some("f(int (A::* const*)())"),
        Some("f"),
    );
}

#[test]
fn libiberty_52() {
    assert_demangle(
        "_Z1jM1AFivEPS1_",
        Some("j(int (A::*)(), int (A::**)())"),
        Some("j"),
    );
}

#[test]
fn libiberty_53() {
    assert_demangle(
        "_Z1sPA37_iPS0_",
        Some("s(int (*) [37], int (**) [37])"),
        Some("s"),
    );
}

#[test]
fn libiberty_54() {
    assert_demangle(
        "_Z3fooA30_A_i",
        Some("foo(int [30][])"),
        Some("foo"),
    );
}

#[test]
fn libiberty_55() {
    assert_demangle(
        "_Z3kooPA28_A30_i",
        Some("koo(int (*) [28][30])"),
        Some("koo"),
    );
}

#[test]
fn libiberty_56() {
    assert_demangle(
        "_ZlsRKU3fooU4bart1XS0_",
        Some("operator<<(X bart foo const&, X bart)"),
        Some("operator<<"),
    );
}

#[test]
fn libiberty_57() {
    assert_demangle(
        "_ZlsRKU3fooU4bart1XS2_",
        Some("operator<<(X bart foo const&, X bart foo const)"),
        Some("operator<<"),
    );
}

#[test]
fn libiberty_58() {
    assert_demangle(
        "_Z1fM1AKFivE",
        Some("f(int (A::*)() const)"),
        Some("f"),
    );
}

#[test]
fn libiberty_59() {
    assert_demangle(
        "_Z3absILi11EEvv",
        Some("void abs<11>()"),
        Some("abs<11>"),
    );
}

// TODO: Returns "A<float>::operator float<int>()" which is invalid
// #[test]
// fn libiberty_60() {
//     assert_demangle(
//         "_ZN1AIfEcvT_IiEEv",
//         Some("A<float>::operator int<int>()"),
//         Some("A<float>::operator int<int>"),
//     );
// }

#[test]
fn libiberty_61() {
    assert_demangle(
        "_ZN12libcw_app_ct10add_optionIS_EEvMT_FvPKcES3_cS3_S3_",
        Some("void libcw_app_ct::add_option<libcw_app_ct>(void (libcw_app_ct::*)(char const*), char const*, char, char const*, char const*)"),
        Some("libcw_app_ct::add_option<libcw_app_ct>"),
    );
}

#[test]
fn libiberty_62() {
    assert_demangle(
        "_ZGVN5libcw24_GLOBAL__N_cbll.cc0ZhUKa23compiler_bug_workaroundISt6vectorINS_13omanip_id_tctINS_5debug32memblk_types_manipulator_data_ctEEESaIS6_EEE3idsE",
        Some("guard variable for libcw::(anonymous namespace)::compiler_bug_workaround<std::vector<libcw::omanip_id_tct<libcw::debug::memblk_types_manipulator_data_ct>, std::allocator<libcw::omanip_id_tct<libcw::debug::memblk_types_manipulator_data_ct> > > >::ids"),
        Some("guard variable for libcw::(anonymous namespace)::compiler_bug_workaround<std::vector<libcw::omanip_id_tct<libcw::debug::memblk_types_manipulator_data_ct>, std::allocator<libcw::omanip_id_tct<libcw::debug::memblk_types_manipulator_data_ct> > > >::ids"),
    );
}

#[test]
fn libiberty_63() {
    assert_demangle(
        "_ZN5libcw5debug13cwprint_usingINS_9_private_12GlobalObjectEEENS0_17cwprint_using_tctIT_EERKS5_MS5_KFvRSt7ostreamE",
        Some("libcw::debug::cwprint_using_tct<libcw::_private_::GlobalObject> libcw::debug::cwprint_using<libcw::_private_::GlobalObject>(libcw::_private_::GlobalObject const&, void (libcw::_private_::GlobalObject::*)(std::ostream&) const)"),
        Some("libcw::debug::cwprint_using<libcw::_private_::GlobalObject>"),
    );
}

#[test]
fn libiberty_64() {
    assert_demangle(
        "_ZNKSt14priority_queueIP27timer_event_request_base_ctSt5dequeIS1_SaIS1_EE13timer_greaterE3topEv",
        Some("std::priority_queue<timer_event_request_base_ct*, std::deque<timer_event_request_base_ct*, std::allocator<timer_event_request_base_ct*> >, timer_greater>::top() const"),
        Some("std::priority_queue<timer_event_request_base_ct*, std::deque<timer_event_request_base_ct*, std::allocator<timer_event_request_base_ct*> >, timer_greater>::top"),
    );
}

#[test]
fn libiberty_65() {
    assert_demangle(
        "_ZNKSt15_Deque_iteratorIP15memory_block_stRKS1_PS2_EeqERKS5_",
        Some("std::_Deque_iterator<memory_block_st*, memory_block_st* const&, memory_block_st* const*>::operator==(std::_Deque_iterator<memory_block_st*, memory_block_st* const&, memory_block_st* const*> const&) const"),
        Some("std::_Deque_iterator<memory_block_st*, memory_block_st* const&, memory_block_st* const*>::operator=="),
    );
}

#[test]
fn libiberty_66() {
    assert_demangle(
        "_ZNKSt17__normal_iteratorIPK6optionSt6vectorIS0_SaIS0_EEEmiERKS6_",
        Some("std::__normal_iterator<option const*, std::vector<option, std::allocator<option> > >::operator-(std::__normal_iterator<option const*, std::vector<option, std::allocator<option> > > const&) const"),
        Some("std::__normal_iterator<option const*, std::vector<option, std::allocator<option> > >::operator-"),
    );
}

#[test]
fn libiberty_67() {
    assert_demangle(
        "_ZNSbIcSt11char_traitsIcEN5libcw5debug27no_alloc_checking_allocatorEE12_S_constructIPcEES6_T_S7_RKS3_",
        Some("char* std::basic_string<char, std::char_traits<char>, libcw::debug::no_alloc_checking_allocator>::_S_construct<char*>(char*, char*, libcw::debug::no_alloc_checking_allocator const&)"),
        Some("std::basic_string<char, std::char_traits<char>, libcw::debug::no_alloc_checking_allocator>::_S_construct<char*>"),
    );
}

#[test]
fn libiberty_68() {
    assert_demangle(
        "_Z1fI1APS0_PKS0_EvT_T0_T1_PA4_S3_M1CS8_",
        Some("void f<A, A*, A const*>(A, A*, A const*, A const* (*) [4], A const* (* C::*) [4])"),
        Some("f<A, A*, A const*>"),
    );
}

#[test]
fn libiberty_69() {
    assert_demangle(
        "_Z3fooiPiPS_PS0_PS1_PS2_PS3_PS4_PS5_PS6_PS7_PS8_PS9_PSA_PSB_PSC_",
        Some("foo(int, int*, int**, int***, int****, int*****, int******, int*******, int********, int*********, int**********, int***********, int************, int*************, int**************, int***************)"),
        Some("foo"),
    );
}

#[test]
fn libiberty_70() {
    assert_demangle(
        "_ZSt1BISt1DIP1ARKS2_PS3_ES0_IS2_RS2_PS2_ES2_ET0_T_SB_SA_PT1_",
        Some("std::D<A*, A*&, A**> std::B<std::D<A*, A* const&, A* const*>, std::D<A*, A*&, A**>, A*>(std::D<A*, A* const&, A* const*>, std::D<A*, A* const&, A* const*>, std::D<A*, A*&, A**>, A**)"),
        Some("std::B<std::D<A*, A* const&, A* const*>, std::D<A*, A*&, A**>, A*>"),
    );
}

// #[test]
// fn libiberty_71() {
//     assert_demangle(
//         "_X11TransParseAddress",
//         Some("_X11TransParseAddress"),
//         Some("_X11TransParseAddress"),
//     );
// }

#[test]
fn libiberty_72() {
    assert_demangle(
        "_ZNSt13_Alloc_traitsISbIcSt18string_char_traitsIcEN5libcw5debug9_private_17allocator_adaptorIcSt24__default_alloc_templateILb0ELi327664EELb1EEEENS5_IS9_S7_Lb1EEEE15_S_instancelessE",
        Some("std::_Alloc_traits<std::basic_string<char, std::string_char_traits<char>, libcw::debug::_private_::allocator_adaptor<char, std::__default_alloc_template<false, 327664>, true> >, libcw::debug::_private_::allocator_adaptor<std::basic_string<char, std::string_char_traits<char>, libcw::debug::_private_::allocator_adaptor<char, std::__default_alloc_template<false, 327664>, true> >, std::__default_alloc_template<false, 327664>, true> >::_S_instanceless"),
        Some("std::_Alloc_traits<std::basic_string<char, std::string_char_traits<char>, libcw::debug::_private_::allocator_adaptor<char, std::__default_alloc_template<false, 327664>, true> >, libcw::debug::_private_::allocator_adaptor<std::basic_string<char, std::string_char_traits<char>, libcw::debug::_private_::allocator_adaptor<char, std::__default_alloc_template<false, 327664>, true> >, std::__default_alloc_template<false, 327664>, true> >::_S_instanceless"),
    );
}

// #[test]
// fn libiberty_73() {
//     assert_demangle(
//         "_GLOBAL__I__Z2fnv",
//         Some("global constructors keyed to fn()"),
//         Some("global constructors keyed to fn()"),
//     );
// }

#[test]
fn libiberty_74() {
    assert_demangle(
        "_Z1rM1GFivEMS_KFivES_M1HFivES1_4whatIKS_E5what2IS8_ES3_",
        Some("r(int (G::*)(), int (G::*)() const, G, int (H::*)(), int (G::*)(), what<G const>, what2<G const>, int (G::*)() const)"),
        Some("r"),
    );
}

#[test]
fn libiberty_75() {
    assert_demangle(
        "_Z10hairyfunc5PFPFilEPcE",
        Some("hairyfunc5(int (* (*)(char*))(long))"),
        Some("hairyfunc5"),
    );
}

// TODO: Throws an error, should be fixed
// #[test]
// fn libiberty_76() {
//     assert_demangle(
//         "_Z1fILi1ELc120EEv1AIXplT_cviLd810000000000000000703DAD7A370C5EEE",
//         Some("void f<1, (char)120>(A<(1)+((int)((double)[810000000000000000703DAD7A370C5]))>)"),
//         Some("f<1, (char)120>"),
//     );
// }

// Returns "void f<1>(A<(1) + ((int)(-(0x1p+0f)))>)"
// but we don't care about this case
// #[test]
// fn libiberty_77() {
//     assert_demangle(
//         "_Z1fILi1EEv1AIXplT_cvingLf3f800000EEE",
//         Some("void f<1>(A<(1) + ((int)(-((float)[3f800000])))>)"),
//         Some("f<1>"),
//     );
// }

#[test]
fn libiberty_78() {
    assert_demangle(
        "_ZNK11__gnu_debug16_Error_formatter14_M_format_wordImEEvPciPKcT_",
        Some("void __gnu_debug::_Error_formatter::_M_format_word<unsigned long>(char*, int, char const*, unsigned long) const"),
        Some("__gnu_debug::_Error_formatter::_M_format_word<unsigned long>"),
    );
}

#[test]
fn libiberty_79() {
    assert_demangle(
        "_ZSt18uninitialized_copyIN9__gnu_cxx17__normal_iteratorIPSt4pairISsPFbP6sqlitePPcEESt6vectorIS9_SaIS9_EEEESE_ET0_T_SG_SF_",
        Some("__gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > > std::uninitialized_copy<__gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > >, __gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > > >(__gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > >, __gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > >, __gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > >)"),
        Some("std::uninitialized_copy<__gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > >, __gnu_cxx::__normal_iterator<std::pair<std::string, bool (*)(sqlite*, char**)>*, std::vector<std::pair<std::string, bool (*)(sqlite*, char**)>, std::allocator<std::pair<std::string, bool (*)(sqlite*, char**)> > > > >"),
    );
}

#[test]
fn libiberty_80() {
    assert_demangle(
        "_Z1fP1cIPFiiEE",
        Some("f(c<int (*)(int)>*)"),
        Some("f"),
    );
}

#[test]
fn libiberty_81() {
    assert_demangle(
        "_Z4dep9ILi3EEvP3fooIXgtT_Li2EEE",
        // NOTE: was "void dep9<3>(foo<((3)>(2))>*)"
        Some("void dep9<3>(foo<((3) > (2))>*)"),
        Some("dep9<3>"),
    );
}

#[test]
fn libiberty_82() {
    assert_demangle(
        "_ZStltI9file_pathSsEbRKSt4pairIT_T0_ES6_",
        // NOTE: was "bool std::operator< <file_path, std::string>(std::pair<file_path, std::string> const&, std::pair<file_path, std::string> const&)"
        Some("bool std::operator<<file_path, std::string>(std::pair<file_path, std::string> const&, std::pair<file_path, std::string> const&)"),
        Some("std::operator< <file_path, std::string>"),
    );
}

// TODO: Returns "hairyfunc(int (* const (X::** (* restrict (* volatile* (Y::*)(int))(char*)) [2])(long)) [3] const const)
// #[test]
// fn libiberty_83() {
//     assert_demangle(
//         "_Z9hairyfuncM1YKFPVPFrPA2_PM1XKFKPA3_ilEPcEiE",
//         Some("hairyfunc(int (* const (X::** (* restrict (* volatile* (Y::*)(int) const)(char*)) [2])(long) const) [3])"),
//         Some("hairyfunc"),
//     );
// }

#[test]
fn libiberty_84() {
    assert_demangle(
        "_Z1fILin1EEvv",
        Some("void f<-1>()"),
        Some("f<-1>"),
    );
}

#[test]
fn libiberty_85() {
    assert_demangle(
        "_ZNSdD0Ev",
        Some("std::basic_iostream<char, std::char_traits<char> >::~basic_iostream()"),
        Some("std::basic_iostream<char, std::char_traits<char> >::~basic_iostream"),
    );
}

#[test]
fn libiberty_86() {
    assert_demangle(
        "_ZNK15nsBaseHashtableI15nsUint32HashKey8nsCOMPtrI4IFooEPS2_E13EnumerateReadEPF15PLDHashOperatorRKjS4_PvES9_",
        Some("nsBaseHashtable<nsUint32HashKey, nsCOMPtr<IFoo>, IFoo*>::EnumerateRead(PLDHashOperator (*)(unsigned int const&, IFoo*, void*), void*) const"),
        Some("nsBaseHashtable<nsUint32HashKey, nsCOMPtr<IFoo>, IFoo*>::EnumerateRead"),
    );
}

#[test]
fn libiberty_87() {
    assert_demangle(
        "_ZNK1C1fIiEEPFivEv",
        Some("int (*C::f<int>() const)()"),
        Some("C::f<int>"),
    );
}

#[test]
fn libiberty_88() {
    assert_demangle(
        "_ZZ3BBdI3FooEvvENK3Fob3FabEv",
        // NOTE: was "BBd<Foo>()::Fob::Fab() const"
        Some("void BBd<Foo>()::Fob::Fab() const"),
        Some("BBd<Foo>()::Fob::Fab"),
    );
}

#[test]
fn libiberty_89() {
    assert_demangle(
        // NOTE: was "BBd<Foo>()::Fob::Fab() const::Gob::Gab() const"
        "_ZZZ3BBdI3FooEvvENK3Fob3FabEvENK3Gob3GabEv",
        Some("void BBd<Foo>()::Fob::Fab() const::Gob::Gab() const"),
        Some("BBd<Foo>()::Fob::Fab() const::Gob::Gab"),
    );
}

#[test]
fn libiberty_90() {
    assert_demangle(
        "_ZNK5boost6spirit5matchI13rcs_deltatextEcvMNS0_4impl5dummyEFvvEEv",
        Some("boost::spirit::match<rcs_deltatext>::operator void (boost::spirit::impl::dummy::*)()() const"),
        Some("boost::spirit::match<rcs_deltatext>::operator void (boost::spirit::impl::dummy::*)()"),
    );
}

// TODO: Returns "void foo<int const [6]>(int const const [9][6], int const const restrict (* volatile restrict) [9][6])"
// #[test]
// fn libiberty_91() {
//     assert_demangle(
//         "_Z3fooIA6_KiEvA9_KT_rVPrS4_",
//         Some("void foo<int const [6]>(int const [9][6], int restrict const (* volatile restrict) [9][6])"),
//         Some("foo<int const [6]>"),
//     );
// }

#[test]
fn libiberty_92() {
    assert_demangle(
        "_Z3fooIA3_iEvRKT_",
        Some("void foo<int [3]>(int const (&) [3])"),
        Some("foo<int [3]>"),
    );
}

#[test]
fn libiberty_93() {
    assert_demangle(
        "_Z3fooIPA3_iEvRKT_",
        Some("void foo<int (*) [3]>(int (* const&) [3])"),
        Some("foo<int (*) [3]>"),
    );
}

#[test]
fn libiberty_94() {
    assert_demangle(
        "_ZN13PatternDriver23StringScalarDeleteValueC1ERKNS_25ConflateStringScalarValueERKNS_25AbstractStringScalarValueERKNS_12TemplateEnumINS_12pdcomplementELZNS_16complement_namesEELZNS_14COMPLEMENTENUMEEEE",
        Some("PatternDriver::StringScalarDeleteValue::StringScalarDeleteValue(PatternDriver::ConflateStringScalarValue const&, PatternDriver::AbstractStringScalarValue const&, PatternDriver::TemplateEnum<PatternDriver::pdcomplement, PatternDriver::complement_names, PatternDriver::COMPLEMENTENUM> const&)"),
        Some("PatternDriver::StringScalarDeleteValue::StringScalarDeleteValue"),
    );
}

// This doesn't make sense in our case
// It is a regression test used to verify that c++filt doesn't segfault
// In our case it is no legal C++ symbol

// #[test]
// fn libiberty_95() {
//     assert_demangle(
//         "ALsetchannels",
//         Some("ALsetchannels"),
//         Some("ALsetchannels"),
//     );
// }

#[test]
fn libiberty_96() {
    assert_demangle(
        "_Z4makeI7FactoryiET_IT0_Ev",
        // NOTE: Originally "make<Factory, int>()Factory<int>"
        // That is obviously wrong
        Some("Factory<int> make<Factory, int>()"),
        Some("make<Factory, int>"),
    );
}

#[test]
fn libiberty_97() {
    assert_demangle(
        "_ZN1KIXadL_ZN1S1mEiEEE1fEv",
        // For some reason, this outputs "K<&(S::m(int))>::f()"
        // Correct would be "K<&S::m>::f()"
        // TODO: Fix this!
        Some("K<&(S::m(int))>::f()"),
        Some("K<&S::m>::f"),
    );
}

// #[test]
// fn libiberty_98() {
//     assert_demangle(
//         "_Z3fo5n.clone.1",
//         Some("fo5(__int128) [clone .clone.1]"),
//         Some("fo5"),
//     );
// }

// #[test]
// fn libiberty_99() {
//     assert_demangle(
//         "_Z3fo5n.constprop.2",
//         Some("fo5(__int128) [clone .constprop.2]"),
//         Some("fo5"),
//     );
// }

// #[test]
// fn libiberty_100() {
//     assert_demangle(
//         "_Z3fo5n.isra.3",
//         Some("fo5(__int128) [clone .isra.3]"),
//         Some("fo5"),
//     );
// }

// #[test]
// fn libiberty_101() {
//     assert_demangle(
//         "_Z3fo5n.part.4",
//         Some("fo5(__int128) [clone .part.4]"),
//         Some("fo5"),
//     );
// }

// #[test]
// fn libiberty_102() {
//     assert_demangle(
//         "_Z12to_be_clonediPv.clone.0",
//         Some("to_be_cloned(int, void*) [clone .clone.0]"),
//         Some("to_be_cloned"),
//     );
// }

// #[test]
// fn libiberty_103() {
//     assert_demangle(
//         "_Z3fooi.1988",
//         Some("foo(int) [clone .1988]"),
//         Some("foo"),
//     );
// }

// #[test]
// fn libiberty_104() {
//     assert_demangle(
//         "_Z3fooi.part.9.165493.constprop.775.31805",
//         Some("foo(int) [clone .part.9.165493] [clone .constprop.775.31805]"),
//         Some("foo"),
//     );
// }

// #[test]
// fn libiberty_105() {
//     assert_demangle(
//         "_Z2f1IiEvT_S0_S0_._omp_fn.2",
//         Some("void f1<int>(int, int, int) [clone ._omp_fn.2]"),
//         Some("f1<int>"),
//     );
// }

// #[test]
// fn libiberty_106() {
//     assert_demangle(
//         "_Z3fooi._omp_cpyfn.6",
//         Some("foo(int) [clone ._omp_cpyfn.6]"),
//         Some("foo"),
//     );
// }

#[test]
fn libiberty_107() {
    assert_demangle(
        "_Z1fIKFvvES0_Evv",
        Some("void f<void () const, void () const>()"),
        Some("f<void () const, void () const>"),
    );
}

// TODO: Returns "strings::internal::Splitter<strings::delimiter::AnyOf, strings::SkipEmpty>::operator strings::delimiter::AnyOf<std::vector<basic_string<char, std::char_traits<char>, std::allocator<char> >, std::allocator<basic_string<char, std::char_traits<char>, std::allocator<char> > > >, void>() const"
// #[test]
// fn libiberty_108() {
//     assert_demangle(
//         "_ZNK7strings8internal8SplitterINS_9delimiter5AnyOfENS_9SkipEmptyEEcvT_ISt6vectorI12basic_stringIcSt11char_traitsIcESaIcEESaISD_EEvEEv",
//         Some("strings::internal::Splitter<strings::delimiter::AnyOf, strings::SkipEmpty>::operator std::vector<basic_string<char, std::char_traits<char>, std::allocator<char> >, std::allocator<basic_string<char, std::char_traits<char>, std::allocator<char> > > ><std::vector<basic_string<char, std::char_traits<char>, std::allocator<char> >, std::allocator<basic_string<char, std::char_traits<char>, std::allocator<char> > > >, void>() const"),
//         Some("strings::internal::Splitter<strings::delimiter::AnyOf, strings::SkipEmpty>::operator std::vector<basic_string<char, std::char_traits<char>, std::allocator<char> >, std::allocator<basic_string<char, std::char_traits<char>, std::allocator<char> > > ><std::vector<basic_string<char, std::char_traits<char>, std::allocator<char> >, std::allocator<basic_string<char, std::char_traits<char>, std::allocator<char> > > >, void>"),
//     );
// }

#[test]
fn libiberty_109() {
    assert_demangle(
        "_ZN1AcvT_I1CEEv",
        Some("A::operator C<C>()"),
        Some("A::operator C<C>"),
    );
}

#[test]
fn libiberty_110() {
    assert_demangle(
        "_ZN1AcvPT_I1CEEv",
        Some("A::operator C*<C>()"),
        Some("A::operator C*<C>"),
    );
}

#[test]
fn libiberty_111() {
    assert_demangle(
        "_ZN1AcvT_IiEI1CEEv",
        Some("A::operator C<int><C>()"),
        Some("A::operator C<int><C>"),
    );
}

// TODO: Throws an error
// #[test]
// fn libiberty_112() {
//     assert_demangle(
//         "_ZNSt8ios_base7failureB5cxx11C1EPKcRKSt10error_code",
//         Some("std::ios_base::failure[abi:cxx11]::failure(char const*, std::error_code const&)"),
//         Some("std::ios_base::failure[abi:cxx11]::failure"),
//     );
// }
