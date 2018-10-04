//! MSVC C++ Demangling Tests
//! We use msvc_demangler under the hood which runs its own test suite.
//! Tests here make it easier to detect regressions.

extern crate symbolic_common;
extern crate symbolic_demangle;
mod utils;

use symbolic_common::types::Language;
use utils::assert_demangle;

// These symbols were extracted from electron.exe.pdb
// https://github.com/electron/electron/releases/download/v2.0.11/electron-v2.0.11-win32-x64-pdb.zip

// NOTE: msvc_demangler cannot demangle without qualifiers and argument lists yet.

#[test]
fn test_operator_delete() {
    assert_demangle(
        Language::Cpp,
        "??3@YAXPEAX@Z",
        Some("void __cdecl operator delete(void*)"),
        Some("void __cdecl operator delete(void*)"),
    );
}

#[test]
fn test_method() {
    assert_demangle(
        Language::Cpp,
        "?LoadV8Snapshot@V8Initializer@gin@@SAXXZ",
        Some("public: static void __cdecl gin::V8Initializer::LoadV8Snapshot(void)"),
        Some("public: static void __cdecl gin::V8Initializer::LoadV8Snapshot(void)"),
    );
}

#[test]
fn test_operator() {
    assert_demangle(
        Language::Cpp,
        "??9@YA_NAEBVGURL@@0@Z",
        Some("bool __cdecl operator!=(class GURL const&,class GURL const&)"),
        Some("bool __cdecl operator!=(class GURL const&,class GURL const&)"),
    );
}

#[test]
fn test_anonymous_namespace() {
    assert_demangle(
        Language::Cpp,
        "??_GAtomSandboxedRenderFrameObserver@?A0x77c58568@atom@@UEAAPEAXI@Z",
        Some("public: virtual void* __cdecl atom::`anonymous namespace`::AtomSandboxedRenderFrameObserver::`scalar deleting destructor'(unsigned int)"),
        Some("public: virtual void* __cdecl atom::`anonymous namespace`::AtomSandboxedRenderFrameObserver::`scalar deleting destructor'(unsigned int)"),
    );
}
