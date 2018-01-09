use symbolic_common::{Language, Name};
use symbolic_demangle::{Demangle, DemangleFormat, DemangleOptions};

const WITH_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: true,
};

const WITHOUT_ARGS: DemangleOptions = DemangleOptions {
    format: DemangleFormat::Short,
    with_arguments: false,
};

pub fn assert_demangle(
    language: Language,
    input: &str,
    with_args: Option<&str>,
    without_args: Option<&str>,
) {
    let name = Name::with_language(input, language);
    if let Some(rv) = name.demangle(WITH_ARGS).unwrap() {
        assert_eq!(Some(rv.as_str()), with_args);
    } else {
        assert_eq!(None, with_args);
    }

    if let Some(rv) = name.demangle(WITHOUT_ARGS).unwrap() {
        assert_eq!(Some(rv.as_str()), without_args);
    } else {
        assert_eq!(None, without_args);
    }
}
