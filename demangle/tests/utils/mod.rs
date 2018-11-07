use symbolic_demangle::{DemangleFormat, DemangleOptions};

#[allow(unused)]
pub const WITH_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: true,
};

#[allow(unused)]
pub const WITHOUT_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: false,
};

#[macro_export]
macro_rules! assert_demangle {
    ($l:expr, $o:expr, { $($m:expr => $d:expr),* }) => {{
        let mut __failures: Vec<String> = Vec::new();

        $({
            use symbolic_demangle::Demangle;

            let __mangled = $m;
            let __demangled = symbolic_common::types::Name::with_language(__mangled, $l).demangle($o);
            let __demangled = __demangled.as_ref().map(|s| s.as_str()).unwrap_or("<demangling failed>");

            if __demangled != $d {
                __failures.push(format!(
                    "{}\n   expected: {}\n   actual:   {}",
                    __mangled,
                    $d,
                    __demangled
                ));
            }
        })*

        if !__failures.is_empty() {
            panic!("demangling failed: \n\n{}\n", __failures.join("\n\n"));
        }
    }};
    ($l:expr, $o:expr, { $($m:expr => $d:expr,)* }) => {
        assert_demangle!($l, $o, { $($m => $d),* })
    };
}
