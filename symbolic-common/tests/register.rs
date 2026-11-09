//! Tests for register types.

use minidump::CpuContext as _;
use minidump::format::{CONTEXT_AMD64, CONTEXT_ARM, CONTEXT_ARM64, CONTEXT_X86};
use symbolic_common::register::{Amd64Register, Arm32Register, Arm64Register, Intel32Register};

#[test]
fn parses_minidump_amd_64_registers() {
    for &register in CONTEXT_AMD64::REGISTERS {
        register.parse::<Amd64Register>().unwrap_or_else(|e| {
            panic!("Failed to parse minidump register {register:?} to Amd64Register: {e}")
        });
    }
}

#[test]
fn parses_minidump_arm_64_registers() {
    for &register in CONTEXT_ARM64::REGISTERS {
        register.parse::<Arm64Register>().unwrap_or_else(|e| {
            panic!("Failed to parse minidump register {register:?} to Arm64Register: {e}")
        });
    }
}

#[test]
fn parses_minidump_x86_registers() {
    for &register in CONTEXT_X86::REGISTERS {
        register.parse::<Intel32Register>().unwrap_or_else(|e| {
            panic!("Failed to parse minidump register {register:?} to Intel32Register: {e}")
        });
    }
}

#[test]
fn parses_minidump_arm_registers() {
    for &register in CONTEXT_ARM::REGISTERS {
        register.parse::<Arm32Register>().unwrap_or_else(|e| {
            panic!("Failed to parse minidump register {register:?} to Arm32Register: {e}")
        });
    }
}
