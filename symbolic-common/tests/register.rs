//! Tests for register types.

use minidump::CpuContext as _;
use minidump::format::CONTEXT_AMD64;
use symbolic_common::register::Amd64Register;

#[test]
fn parses_minidump_amd_64_registers() {
    for &register in CONTEXT_AMD64::REGISTERS {
        register.parse::<Amd64Register>().unwrap_or_else(|e| {
            panic!("Failed to parse minidump register {register:?} to Amd64Register: {e}")
        });
    }
}
