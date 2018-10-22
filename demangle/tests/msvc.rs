//! MSVC C++ Demangling Tests
//! We use msvc_demangler under the hood which runs its own test suite.
//! Tests here make it easier to detect regressions.

extern crate symbolic_common;
extern crate symbolic_demangle;
#[macro_use]
mod utils;

use symbolic_common::types::Language;

#[test]
fn test_msvc_demangle() {
    assert_demangle!(Language::Cpp, utils::WITH_ARGS, {
        // These symbols were extracted from electron.exe.pdb
        // https://github.com/electron/electron/releases/download/v2.0.11/electron-v2.0.11-win32-x64-pdb.zip
        "??3@YAXPEAX@Z" => "void __cdecl operator delete(void*)",
        "?LoadV8Snapshot@V8Initializer@gin@@SAXXZ" => "public: static void __cdecl gin::V8Initializer::LoadV8Snapshot(void)",
        "??9@YA_NAEBVGURL@@0@Z" => "bool __cdecl operator!=(class GURL const&,class GURL const&)",
        "??_GAtomSandboxedRenderFrameObserver@?A0x77c58568@atom@@UEAAPEAXI@Z" => "public: virtual void* __cdecl atom::`anonymous namespace`::AtomSandboxedRenderFrameObserver::`scalar deleting destructor'(unsigned int)",
    })
}

// NOTE: msvc_demangler cannot demangle without qualifiers and argument lists yet.
