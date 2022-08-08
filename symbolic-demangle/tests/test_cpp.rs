//! C++ Itanium Demangling Tests
//! We use cpp_demangle under the hood which runs the libiberty test suite
//! Still, we run some basic regression tests here to detect demangling differences.

#![cfg(feature = "cpp")]

#[macro_use]
mod utils;

use symbolic_common::Language;
use symbolic_demangle::DemangleOptions;

#[test]
fn test_demangle_cpp() {
    assert_demangle!(Language::Cpp, DemangleOptions::name_only().parameters(true), {
        "_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE" => "JS_GetPropertyDescriptorById(JSContext*, JS::Handle<JSObject*>, JS::Handle<jsid>, JS::MutableHandle<JS::PropertyDescriptor>)",
        "_ZN12_GLOBAL__N_15startEv" => "(anonymous namespace)::start()",
        "__ZN12_GLOBAL__N_15startEv" => "(anonymous namespace)::start()",
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv" => "(anonymous namespace)::hello()::$_0::operator()() const",
        "_Z3MinIiiEDTqultfp_fp0_cl7forwardIT_Efp_Ecl7forwardIT0_Efp0_EEOS0_OS1_" => "Min<int, int>(int&&, int&&)",
        "___ZN19URLConnectionClient33_clientInterface_cancelConnectionEP16dispatch_queue_sU13block_pointerFvvE_block_invoke14" => "invocation function for block in URLConnectionClient::_clientInterface_cancelConnection(dispatch_queue_s*, void () block_pointer)",

        // Broken in cpp_demangle
        // "_ZN4base8internal13FunctorTraitsIPFvvEvE6InvokeIJEEEvS3_DpOT_" => "void base::internal::FunctorTraits<void (*)(), void>::Invoke<>(void (*)())",
    });
}

#[test]
fn test_demangle_cpp_no_args() {
    assert_demangle!(Language::Cpp, DemangleOptions::name_only(), {
        "_Z28JS_GetPropertyDescriptorByIdP9JSContextN2JS6HandleIP8JSObjectEENS2_I4jsidEENS1_13MutableHandleINS1_18PropertyDescriptorEEE" => "JS_GetPropertyDescriptorById",
        "_ZN12_GLOBAL__N_15startEv" => "(anonymous namespace)::start",
        "_ZZN12_GLOBAL__N_15helloEvENK3$_0clEv" => "(anonymous namespace)::hello()::$_0::operator() const",
        "___ZN19URLConnectionClient33_clientInterface_cancelConnectionEP16dispatch_queue_sU13block_pointerFvvE_block_invoke14" => "invocation function for block in URLConnectionClient::_clientInterface_cancelConnection",

        // Broken in cpp_demangle
        // "_ZN4base8internal13FunctorTraitsIPFvvEvE6InvokeIJEEEvS3_DpOT_" => "void base::internal::FunctorTraits<void (*)(), void>::Invoke<>",
        // "_Z3MinIiiEDTqultfp_fp0_cl7forwardIT_Efp_Ecl7forwardIT0_Efp0_EEOS0_OS1_" => "decltype (({parm#1}<{parm#2})?((forward<int>)({parm#1})) : ((forward<int>)({parm#2}))) Min<int, int>",
    });
}

#[test]
fn test_demangle_cpp_hash_suffix() {
    assert_demangle!(Language::Cpp, DemangleOptions::complete(), {
    "__ZZN3xxx12xxxxxxxxxxxx9xxxxxxxxxILNS0_16xxxxxxxxxxxxxxxxE0EZNKS_6xxxxxx16xxxxxxxxxxxxxxxxEPjbbE4$_76EEvRKT0_PS3_PNS_7xxxxxxxENS0_13xxxxxxxxxxxxxEbbEN18xxxxxxxxxxxxxxxxxx10xxxxxxxxxxEv$57c34bde3fedbd1a4bf6fbbe5453ff24" =>
    "void xxx::xxxxxxxxxxxx::xxxxxxxxx<(xxx::xxxxxxxxxxxx::xxxxxxxxxxxxxxxx)0, xxx::xxxxxx::xxxxxxxxxxxxxxxx(unsigned int*, bool, bool) const::$_76>(xxx::xxxxxx::xxxxxxxxxxxxxxxx(unsigned int*, bool, bool) const::$_76 const&, xxx::xxxxxx*, xxx::xxxxxxx*, xxx::xxxxxxxxxxxx::xxxxxxxxxxxxx, bool, bool)::xxxxxxxxxxxxxxxxxx::xxxxxxxxxx()"
    });
}

#[test]
fn test_deep_recursion() {
    assert_demangle!(Language::Cpp, DemangleOptions::complete(), {
        "_ZNK8xxxxxxxx14xxxxxxxxxxxxxxINS_14xxxxxxxxxxxxxxINS1_INS1_INS1_INS1_INS1_INS_10xxxxxxxxxxE32xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxEES3_EE22xxxxxxxxxxxxxxxxxxxxxxEES6_EE13xxxxxxxxxxxxxEE17xxxxxxxxxxxxxxxxxEE14xxxxxxxxxxxxxxE8xxxxxxxxE7xxxxxxxRKN4xxxx5xxxxxEb" =>
        "xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxxxxxx<xxxxxxxx::xxxxxxxxxx, xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx>, xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx>, xxxxxxxxxxxxxxxxxxxxxx>, xxxxxxxxxxxxxxxxxxxxxx>, xxxxxxxxxxxxx>, xxxxxxxxxxxxxxxxx>, xxxxxxxxxxxxxx>::xxxxxxxx(xxxxxxx, xxxx::xxxxx const&, bool) const",

        "_ZN5boost7variantIiJljdbNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEENS_17basic_string_viewIcS4_EElmxyNS_10filesystem4pathEEEC2IS6_EEOT_PNS_9enable_ifINS_3mpl3or_INSG_4and_INS_19is_rvalue_referenceISE_EENSG_4not_INS_8is_constISD_EEEENSL_INS_7is_sameISD_SB_EEEENS_6detail7variant29is_variant_constructible_fromISE_NSG_6l_itemIN4mpl_5long_ILl12EEEiNSV_INSX_ILl11EEElNSV_INSX_ILl10EEEjNSV_INSX_ILl9EEEdNSV_INSX_ILl8EEEbNSV_INSX_ILl7EEES6_NSV_INSX_ILl6EEES8_NSV_INSX_ILl5EEElNSV_INSX_ILl4EEEmNSV_INSX_ILl3EEExNSV_INSX_ILl2EEEyNSV_INSX_ILl1EEESA_NSG_5l_endEEEEEEEEEEEEEEEEEEEEEEEEEEENSW_5bool_ILb1EEEEENSP_ISD_NS_18recursive_variant_EEENS1O_ILb0EEES1T_S1T_EEvE4typeE" =>
        "boost::variant<int, long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::variant<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> > >(boost::enable_if<boost::mpl::or_<boost::mpl::and_<boost::is_rvalue_reference<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >&&>, boost::mpl::not_<boost::is_const<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> > > >, boost::mpl::not_<boost::is_same<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::variant<int, long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >, boost::detail::variant::is_variant_constructible_from<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >&&, boost::mpl::l_item<mpl_::long_<(long)12>, int, boost::mpl::l_item<mpl_::long_<(long)11>, long, boost::mpl::l_item<mpl_::long_<(long)10>, unsigned int, boost::mpl::l_item<mpl_::long_<(long)9>, double, boost::mpl::l_item<mpl_::long_<(long)8>, bool, boost::mpl::l_item<mpl_::long_<(long)7>, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::mpl::l_item<mpl_::long_<(long)6>, boost::basic_string_view<char, std::char_traits<char> >, boost::mpl::l_item<mpl_::long_<(long)5>, long, boost::mpl::l_item<mpl_::long_<(long)4>, unsigned long, boost::mpl::l_item<mpl_::long_<(long)3>, long long, boost::mpl::l_item<mpl_::long_<(long)2>, unsigned long long, boost::mpl::l_item<mpl_::long_<(long)1>, boost::filesystem::path, boost::mpl::l_end> > > > > > > > > > > > >, mpl_::bool_<true> >, boost::is_same<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::recursive_variant_>, mpl_::bool_<false>, mpl_::bool_<false>, mpl_::bool_<false> >, void>::type*)",

        "_ZN5boost6detail7variant21make_initializer_node5applyINS_3mpl4pairINS3_INS5_INS3_INS5_INS3_INS5_INS3_INS5_INS3_INS5_INS1_16initializer_rootEN4mpl_4int_ILi0EEEEENS4_6l_iterINS4_6list12IixjdbNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEENS_17basic_string_viewIcSG_EElmxyNS_10filesystem4pathEEEEEE16initializer_nodeENS8_ILi1EEEEENSB_INS4_6list11IxjdbSI_SK_lmxySM_EEEEE16initializer_nodeENS8_ILi2EEEEENSB_INS4_6list10IjdbSI_SK_lmxySM_EEEEE16initializer_nodeENS8_ILi3EEEEENSB_INS4_5list9IdbSI_SK_lmxySM_EEEEE16initializer_nodeENS8_ILi4EEEEENSB_INS4_5list8IbSI_SK_lmxySM_EEEEE16initializer_nodeENS8_ILi5EEEEENSB_INS4_5list7ISI_SK_lmxySM_EEEEE16initializer_node10initializeEPvRKSI_" =>
        "boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::make_initializer_node::apply<boost::mpl::pair<boost::detail::variant::initializer_root, mpl_::int_<0> >, boost::mpl::l_iter<boost::mpl::list12<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node, mpl_::int_<1> >, boost::mpl::l_iter<boost::mpl::list11<long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node, mpl_::int_<2> >, boost::mpl::l_iter<boost::mpl::list10<unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node, mpl_::int_<3> >, boost::mpl::l_iter<boost::mpl::list9<double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node, mpl_::int_<4> >, boost::mpl::l_iter<boost::mpl::list8<bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node, mpl_::int_<5> >, boost::mpl::l_iter<boost::mpl::list7<std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path> > >::initializer_node::initialize(void*, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> > const&)",

        "_ZN5boost6detail7variant15visitation_implIN4mpl_4int_ILi0EEENS1_20visitation_impl_stepINS_3mpl6l_iterINS7_6l_itemINS3_5long_ILl12EEEiNS9_INSA_ILl11EEExNS9_INSA_ILl10EEEjNS9_INSA_ILl9EEEdNS9_INSA_ILl8EEEbNS9_INSA_ILl7EEENSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEENS9_INSA_ILl6EEENS_17basic_string_viewIcSK_EENS9_INSA_ILl5EEElNS9_INSA_ILl4EEEmNS9_INSA_ILl3EEExNS9_INSA_ILl2EEEyNS9_INSA_ILl1EEENS_10filesystem4pathENS7_5l_endEEEEEEEEEEEEEEEEEEEEEEEEEEENS8_ISX_EEEENS_7variantIiJxjdbSM_SP_lmxySW_EE8assignerEPKvNS1E_18has_fallback_type_EEENT1_11result_typeEiiRS1J_T2_NS3_5bool_ILb0EEET3_PT_PT0_" =>
        "boost::variant<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::assigner::result_type boost::detail::variant::visitation_impl<mpl_::int_<0>, boost::detail::variant::visitation_impl_step<boost::mpl::l_iter<boost::mpl::l_item<mpl_::long_<(long)12>, int, boost::mpl::l_item<mpl_::long_<(long)11>, long long, boost::mpl::l_item<mpl_::long_<(long)10>, unsigned int, boost::mpl::l_item<mpl_::long_<(long)9>, double, boost::mpl::l_item<mpl_::long_<(long)8>, bool, boost::mpl::l_item<mpl_::long_<(long)7>, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::mpl::l_item<mpl_::long_<(long)6>, boost::basic_string_view<char, std::char_traits<char> >, boost::mpl::l_item<mpl_::long_<(long)5>, long, boost::mpl::l_item<mpl_::long_<(long)4>, unsigned long, boost::mpl::l_item<mpl_::long_<(long)3>, long long, boost::mpl::l_item<mpl_::long_<(long)2>, unsigned long long, boost::mpl::l_item<mpl_::long_<(long)1>, boost::filesystem::path, boost::mpl::l_end> > > > > > > > > > > > >, boost::mpl::l_iter<boost::mpl::l_end> >, boost::variant<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::assigner, void const*, boost::variant<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::has_fallback_type_>(int, int, boost::variant<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::assigner&, void const*, mpl_::bool_<false>, boost::variant<int, long long, unsigned int, double, bool, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::basic_string_view<char, std::char_traits<char> >, long, unsigned long, long long, unsigned long long, boost::filesystem::path>::has_fallback_type_, mpl_::int_<0>*, boost::detail::variant::visitation_impl_step<boost::mpl::l_iter<boost::mpl::l_item<mpl_::long_<(long)12>, int, boost::mpl::l_item<mpl_::long_<(long)11>, long long, boost::mpl::l_item<mpl_::long_<(long)10>, unsigned int, boost::mpl::l_item<mpl_::long_<(long)9>, double, boost::mpl::l_item<mpl_::long_<(long)8>, bool, boost::mpl::l_item<mpl_::long_<(long)7>, std::__cxx11::basic_string<char, std::char_traits<char>, std::allocator<char> >, boost::mpl::l_item<mpl_::long_<(long)6>, boost::basic_string_view<char, std::char_traits<char> >, boost::mpl::l_item<mpl_::long_<(long)5>, long, boost::mpl::l_item<mpl_::long_<(long)4>, unsigned long, boost::mpl::l_item<mpl_::long_<(long)3>, long long, boost::mpl::l_item<mpl_::long_<(long)2>, unsigned long long, boost::mpl::l_item<mpl_::long_<(long)1>, boost::filesystem::path, boost::mpl::l_end> > > > > > > > > > > > >, boost::mpl::l_iter<boost::mpl::l_end> >*)",
    });
}

// See https://github.com/getsentry/symbolic/issues/477
// Not fully fixed, so skip for now :-(
// #[test]
// fn test_bounded_buf() {
//     let s = "_ZUlzjjlZZL1zStUlSt7j_Z3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjfjzL4t3kjIIjfjtUlSt7j_Z3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjfjzL4t3kjIIjfjzL4t7IjIjjzjjzSt7j_Z3kjIIjfjzStfjzSt7j_ZA3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjfjzL4t3kjIIjzL4t7IjIjjzjjzSt7j_Z3kjIIjfjzStfjzSt7j_ZA3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjIjL1vfIIEEEjzjjfjzSt7j_Z3kjIIjfjzL4t3kjIIjfjzL4t7IjIjL1vfIIEEEjzjjSI";
//     assert_eq!(
//         symbolic_demangle::demangle(s),
//         std::borrow::Cow::Borrowed(s)
//     );
// }
