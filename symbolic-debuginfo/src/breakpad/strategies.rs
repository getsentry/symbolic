use proptest::prelude::*;

prop_compose! {
    pub(crate) fn arb_module_record()(
        os in "Linux|mac|windows",
        arch in "x86(_64)?|ppc(_64)?|unknown",
        id in "[a-fA-F0-9]{32,40}",
        name in "[^\n]{30,40}",
    ) -> String {
        format!("MODULE {} {} {} {}", os, arch, id, name)
    }
}

prop_compose! {
    pub(crate) fn arb_file_record()(
        id in any::<u64>(),
        name in "[^\n]{0,10}",
    ) -> String {
        format!("FILE {} {}", id, name)
    }
}

prop_compose! {
    pub(crate) fn arb_func_record()(
        multiple in "(m )?",
        address in any::<u64>(),
        size in any::<u64>(),
        parameter_size in any::<u64>(),
        name in "[^\n]{30,40}",
    ) -> String {
        format!("FUNC {}{:x} {:x} {:x} {}", multiple, address, size, parameter_size, name)
    }
}

prop_compose! {
    pub(crate) fn arb_line_record()(
        address in any::<u64>(),
        size in any::<u64>(),
        line in any::<u64>(),
        file_id in any::<u64>(),
    ) -> String {
        format!("{:x} {:x} {} {}", address, size, line, file_id)
    }
}

prop_compose! {
    pub(crate) fn arb_public_record()(
        multiple in "(m )?",
        address in any::<u64>(),
        parameter_size in any::<u64>(),
        name in "[^\n]{30,40}",
    ) -> String {
        format!("PUBLIC {}{:x} {:x} {}", multiple, address, parameter_size, name)
    }
}

prop_compose! {
    pub(crate) fn arb_stack_win_record()(
    ty in "0|4",
    code_start in any::<u32>(),
    code_size in any::<u32>(),
    prolog_size in any::<u16>(),
    epilog_size in any::<u16>(),
    param_size in any::<u32>(),
    saved_regs_size in any::<u16>(),
    locals_size in any::<u32>(),
    max_stack_size in any::<u32>(),
    program_string in "[^\n]{30,40}",
    ) -> String {
        format!(
            "STACK WIN {} {:x} {:x} {:x} {:x} {:x} {:x} {:x} {:x} 0 1 {}",
            ty,
            code_start,
            code_size,
            prolog_size,
            epilog_size,
            param_size,
            saved_regs_size,
            locals_size,
            max_stack_size,
            program_string
        )
    }
}

prop_compose! {
    pub(crate) fn arb_stack_cfi_init_record()(
        address in any::<u64>(),
        size in any::<u64>(),
        init_rules in "[^\n]{30,40}",
    ) -> String {
        format!("STACK CFI INIT {:x} {:x} {}", address, size, init_rules)
    }
}

prop_compose! {
    pub(crate) fn arb_stack_cfi_delta_record()(
        address in any::<u64>(),
        rules in "[^\n]{30,40}",
    ) -> String {
        format!("STACK CFI {:x} {}", address, rules)
    }
}
