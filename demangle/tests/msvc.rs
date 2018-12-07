//! MSVC C++ Demangling Tests
//! We use msvc_demangler under the hood which runs its own test suite.
//! Tests here make it easier to detect regressions.

extern crate symbolic_common;
extern crate symbolic_demangle;
#[macro_use]
mod utils;

use symbolic_common::types::Language;

#[test]
fn test_msvc_demangle_full() {
    assert_demangle!(Language::Cpp, utils::WITHOUT_ARGS, {
        // These symbols were extracted from electron.exe.pdb
        // https://github.com/electron/electron/releases/download/v2.0.11/electron-v2.0.11-win32-x64-pdb.zip
        "??3@YAXPEAX@Z" => "operator delete",
        "?LoadV8Snapshot@V8Initializer@gin@@SAXXZ" => "gin::V8Initializer::LoadV8Snapshot",
        "??9@YA_NAEBVGURL@@0@Z" => "operator!=",
        "??_GAtomSandboxedRenderFrameObserver@?A0x77c58568@atom@@UEAAPEAXI@Z" => "atom::`anonymous namespace`::AtomSandboxedRenderFrameObserver::`scalar deleting destructor'",
    })
}

#[test]
fn test_msvc_demangle_without_args() {
    assert_demangle!(Language::Cpp, utils::WITH_ARGS, {
        // These symbols were extracted from electron.exe.pdb
        // https://github.com/electron/electron/releases/download/v2.0.11/electron-v2.0.11-win32-x64-pdb.zip
        "??3@YAXPEAX@Z" => "operator delete(void *)",
        "?LoadV8Snapshot@V8Initializer@gin@@SAXXZ" => "gin::V8Initializer::LoadV8Snapshot(void)",
        "??9@YA_NAEBVGURL@@0@Z" => "operator!=(class GURL const &,class GURL const &)",
        "??_GAtomSandboxedRenderFrameObserver@?A0x77c58568@atom@@UEAAPEAXI@Z" => "atom::`anonymous namespace`::AtomSandboxedRenderFrameObserver::`scalar deleting destructor'(unsigned int)",
    })
}

// NOTE: msvc_demangler cannot demangle without qualifiers and argument lists yet.
