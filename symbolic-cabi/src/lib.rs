//! Exposes a C-ABI for symbolic.

#![allow(clippy::missing_safety_doc)]

#[macro_use]
mod utils;

mod common;
mod core;
mod debuginfo;
mod demangle;
mod minidump;
mod proguard;
mod sourcemap;
mod symcache;
mod unreal;

pub use crate::common::*;
pub use crate::core::*;
pub use crate::debuginfo::*;
pub use crate::demangle::*;
pub use crate::minidump::*;
pub use crate::proguard::*;
pub use crate::sourcemap::*;
pub use crate::symcache::*;
pub use crate::unreal::*;
