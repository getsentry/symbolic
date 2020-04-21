use clap::{clap_app, ArgMatches};
use failure::{Error, ResultExt};

use symbolic_common::ByteView;
use symbolic_debuginfo::{Function, Object};
use symbolic_demangle::Demangle;

fn print_error(error: &Error) {
    eprintln!("Error: {}", error);

    for cause in error.iter_causes() {
        eprintln!("   caused by {}", cause);
    }
}

fn resolve(function: &Function<'_>, addr: u64, matches: &ArgMatches<'_>) -> Result<(), Error> {
    if matches.is_present("inlines") {
        for il in &function.inlinees {
            resolve(il, addr, matches)?;
        }
    }

    for line in &function.lines {
        if line.address > addr || line.address + line.size.unwrap_or(1) <= addr {
            continue;
        }

        if matches.is_present("functions") {
            if matches.is_present("demangle") {
                print!("{}", function.name.try_demangle(Default::default()));
            } else {
                print!("{}", function.name);
            }

            if matches.is_present("ranges") {
                print!(
                    " ({:#x} - {:#x})",
                    function.address,
                    function.address + function.size
                );
            }

            print!("\n  at ");
        }

        let file = if matches.is_present("basenames") {
            line.file.name_str()
        } else {
            line.file.path_str().into()
        };

        print!("{}:{}", file, line.line);

        if matches.is_present("ranges") {
            print!(" ({:#x} - ", line.address);
            match line.size {
                Some(size) => print!("{:#x})", line.address + size),
                None => print!("??)"),
            }
        }

        println!();
        break;
    }

    Ok(())
}

fn execute(matches: &ArgMatches<'_>) -> Result<(), Error> {
    let path = matches.value_of("path").unwrap_or("a.out");
    let view = ByteView::open(path).context("failed to open file")?;
    let object = Object::parse(&view).context("failed to parse file")?;
    let session = object.debug_session().context("failed to process file")?;

    for addr in matches.values_of("addrs").unwrap_or_default() {
        let addr = if addr.starts_with("0x") {
            u64::from_str_radix(&addr[2..], 16)
        } else {
            addr.parse()
        }
        .context("unable to parse address")?;

        for function in session.functions() {
            let function = function.context("failed to read function")?;
            resolve(&function, addr, matches)?;
        }
    }

    Ok(())
}

fn main() {
    let matches = clap_app!(addr2line =>
        (about: "addr2line translates addresses into file names and line numbers. Given an address in an executable or an offset in a section of a relocatable object, it uses the debugging information to figure out which file name and line number are associated with it.{n}{n}addr2line has two modes of operation.{n}{n}In the first, hexadecimal addresses are specified on the command line, and addr2line displays the file name and line number for each address.{n}{n}In the second, addr2line reads hexadecimal addresses from standard input, and prints the file name and line number for each address on standard output. In this mode, addr2line may be used in a pipe to convert dynamically chosen addresses.")
        (@arg demangle: -C --demangle "Decode (demangle) low-level symbol names into user-level names. Besides removing any initial underscore prepended by the system, this makes C ++ function names readable.")
        (@arg path: -e --exe +takes_value "Specify the name of the executable for which addresses should be translated. The default file is a.out.")
        (@arg functions: -f --functions "Display function names as well as file and line number information.")
        (@arg ranges: -r --ranges "Display function address ranges in addition to function names.")
        (@arg basenames: -s --basenames "Display only the base of each file name.")
        (@arg inlines: -i --inlinees "If the address belongs to a function that was inlined, the source information for all enclosing scopes back to the first non-inlined function will also be printed. For example, if \"main\" inlines \"callee1\" which inlines \"callee2\", and address is from \"callee2\", the source information for \"callee1\" and \"main\" will also be printed.")
        (@arg addrs: +required ... "Addresses to be translated.")
    )
    .get_matches();

    match execute(&matches) {
        Ok(()) => (),
        Err(e) => print_error(&e),
    };
}
