#[macro_export]
macro_rules! assert_demangle {
    ($l:expr, $o:expr, { $($m:expr => $d:expr),* }) => {{
        let mut __failures: Vec<String> = Vec::new();

        $({
            use symbolic_demangle::Demangle;

            let __mangled = $m;
            let __demangled = ::symbolic_common::Name::new(__mangled, ::symbolic_common::NameMangling::Unknown, $l).demangle($o);
            let __demangled = __demangled.as_ref().map(String::as_str).unwrap_or("<demangling failed>");

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
