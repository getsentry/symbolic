#[cfg(not(feature = "processor"))]
fn main() {
    // NOOP
}

#[cfg(feature = "processor")]
fn main() {
    use std::path::Path;
    use std::process::Command;

    if !Path::new("third_party/breakpad/src").exists() {
        let status = Command::new("git")
            .args(&["submodule", "update", "--init"])
            .status()
            .expect("Failed to install git submodules");

        assert!(status.success(), "Failed to install git submodules");
    }

    cc::Build::new()
        .warnings(false)
        .flag_if_supported("-Wno-tautological-constant-out-of-range-compare")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_implicit.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_insn.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_invariant.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_modrm.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_opcode_tables.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_operand.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_reg.c")
        .file("third_party/breakpad/src/third_party/libdisasm/ia32_settings.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_disasm.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_format.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_imm.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_insn.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_misc.c")
        .file("third_party/breakpad/src/third_party/libdisasm/x86_operand_list.c")
        .compile("disasm");

    cc::Build::new()
        .cpp(true)
        .warnings(false)
        .flag_if_supported("-std=c++11")
        .include(".")
        .include("third_party/breakpad/src")
        .define("BPLOG_MINIMUM_SEVERITY", "SEVERITY_ERROR")
        .define(
            "BPLOG(severity)",
            "1 ? (void)0 : google_breakpad::LogMessageVoidify() & (BPLOG_ERROR)",
        )
        // Processor
        .file("third_party/breakpad/src/processor/basic_code_modules.cc")
        .file("third_party/breakpad/src/processor/basic_source_line_resolver.cc")
        .file("third_party/breakpad/src/processor/call_stack.cc")
        .file("third_party/breakpad/src/processor/cfi_frame_info.cc")
        .file("third_party/breakpad/src/processor/convert_old_arm64_context.cc")
        .file("third_party/breakpad/src/processor/disassembler_x86.cc")
        .file("third_party/breakpad/src/processor/dump_context.cc")
        .file("third_party/breakpad/src/processor/dump_object.cc")
        .file("third_party/breakpad/src/processor/logging.cc")
        .file("third_party/breakpad/src/processor/pathname_stripper.cc")
        .file("third_party/breakpad/src/processor/process_state.cc")
        .file("third_party/breakpad/src/processor/proc_maps_linux.cc")
        .file("third_party/breakpad/src/processor/simple_symbol_supplier.cc")
        .file("third_party/breakpad/src/processor/source_line_resolver_base.cc")
        .file("third_party/breakpad/src/processor/stack_frame_cpu.cc")
        .file("third_party/breakpad/src/processor/stack_frame_symbolizer.cc")
        .file("third_party/breakpad/src/processor/stackwalker.cc")
        .file("third_party/breakpad/src/processor/stackwalker_amd64.cc")
        .file("third_party/breakpad/src/processor/stackwalker_arm.cc")
        .file("third_party/breakpad/src/processor/stackwalker_arm64.cc")
        .file("third_party/breakpad/src/processor/stackwalker_mips.cc")
        .file("third_party/breakpad/src/processor/stackwalker_ppc.cc")
        .file("third_party/breakpad/src/processor/stackwalker_ppc64.cc")
        .file("third_party/breakpad/src/processor/stackwalker_sparc.cc")
        .file("third_party/breakpad/src/processor/stackwalker_x86.cc")
        .file("third_party/breakpad/src/processor/tokenize.cc")
        // Minidump
        .file("third_party/breakpad/src/processor/exploitability.cc")
        .file("third_party/breakpad/src/processor/exploitability_linux.cc")
        .file("third_party/breakpad/src/processor/exploitability_win.cc")
        .file("third_party/breakpad/src/processor/minidump.cc")
        .file("third_party/breakpad/src/processor/minidump_processor.cc")
        .file("third_party/breakpad/src/processor/symbolic_constants_win.cc")
        // Symbolic bindings
        .file("cpp/c_string.cpp")
        .file("cpp/data_structures.cpp")
        .file("cpp/mmap_symbol_supplier.cpp")
        .file("cpp/processor.cpp")
        .compile("breakpad");
}
