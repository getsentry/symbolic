use core::fmt;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command};
use minidump::system_info::PointerWidth;
use minidump::{Minidump, Module};
use minidump_processor::{
    FillSymbolError, FrameSymbolizer, FrameTrust, FrameWalker, ProcessState, StackFrame,
    SymbolFile, SymbolStats,
};
use parking_lot::RwLock;
use thiserror::Error;
use walkdir::WalkDir;

use symbolic::common::{Arch, ByteView, DebugId, InstructionInfo, SelfCell};
use symbolic::debuginfo::{Archive, FileFormat, Object};
use symbolic::demangle::{Demangle, DemangleOptions};
use symbolic::minidump::cfi::CfiCache;
use symbolic::symcache::{Error as SymCacheError, SourceLocation, SymCache, SymCacheConverter};

type CfiFiles = BTreeMap<DebugId, Result<SymbolFile, SymbolError>>;
type SymCaches<'a> = BTreeMap<DebugId, Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError>>;
type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, Copy, Error)]
enum SymbolError {
    #[error("not found")]
    NotFound,
    #[error("corrupt debug file")]
    Corrupt,
    #[error("unknown error")]
    Other,
}

/// A SymbolProvider that recursively searches a given path for symbol files.
struct LocalSymbolProvider<'a> {
    symbols_path: PathBuf,
    cfi_files: RwLock<CfiFiles>,
    symcaches: RwLock<SymCaches<'a>>,
    use_cfi: bool,
    symbolicate: bool,
}

impl<'a> LocalSymbolProvider<'a> {
    /// Constructs a `LocalSymbolProvider` that will look for symbol files under the given path.
    fn new(path: PathBuf, use_cfi: bool, symbolicate: bool) -> Self {
        Self {
            symbols_path: path,
            cfi_files: RwLock::new(BTreeMap::default()),
            symcaches: RwLock::new(SymCaches::default()),
            use_cfi,
            symbolicate,
        }
    }

    /// Consumes this `LocalSymbolProvider` and returns its collections of cfi and debug files.
    fn into_inner(self) -> (CfiFiles, SymCaches<'a>) {
        (self.cfi_files.into_inner(), self.symcaches.into_inner())
    }

    /// Attempt to load CFI for the given debug id.
    #[tracing::instrument(level = "trace", skip_all, fields(id = %id))]
    fn load_cfi(&self, id: DebugId) -> Result<SymbolFile, SymbolError> {
        self.find_object(id, |object| {
            if !object.has_unwind_info() {
                return Err(SymbolError::NotFound);
            }

            let cfi_cache = match CfiCache::from_object(object) {
                Ok(cficache) => cficache,
                Err(e) => {
                    tracing::error!(error = %e);
                    return Err(SymbolError::NotFound);
                }
            };

            if cfi_cache.as_slice().is_empty() {
                return Err(SymbolError::NotFound);
            }

            SymbolFile::from_bytes(cfi_cache.as_slice()).map_err(|_| SymbolError::Corrupt)
        })
    }

    /// Attempt to load symbol information for the given debug id.
    #[tracing::instrument(level = "trace", skip_all, fields(id = %id))]
    fn load_symbol_info(
        &self,
        id: DebugId,
    ) -> Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError> {
        self.find_object(id, |object| {
            // Silently skip all incompatible debug symbols
            if !object.has_debug_info() {
                return Err(SymbolError::NotFound);
            }

            let mut buffer = Vec::new();
            if let Err(e) = SymCacheWriter::write_object(object, Cursor::new(&mut buffer)) {
                tracing::error!(error = %e);
                return Err(SymbolError::Corrupt);
            }

            SelfCell::try_new(ByteView::from_vec(buffer), |ptr| {
                SymCache::parse(unsafe { &*ptr })
            })
            .map_err(|_| SymbolError::Corrupt)
        })
    }

    /// Search for an object file belonging to the given debug id and process it with the given function.
    fn find_object<T, F>(&self, id: DebugId, func: F) -> Result<T, SymbolError>
    where
        F: Fn(&Object) -> Result<T, SymbolError>,
    {
        let mut found = None;

        'outer: for entry in WalkDir::new(&self.symbols_path)
            .into_iter()
            .filter_map(Result::ok)
        {
            // Folders will be recursed into automatically
            if !entry.metadata().map_err(|_| SymbolError::Other)?.is_file() {
                continue;
            }

            // Try to parse a potential object file. If this is not possible, then
            // we're not dealing with an object file, thus silently skipping it
            let buffer = ByteView::open(entry.path()).map_err(|_| SymbolError::Other)?;
            let archive = match Archive::parse(&buffer) {
                Ok(archive) => archive,
                Err(_) => continue,
            };

            for object in archive.objects() {
                // Fail for invalid matching objects but silently skip objects
                // without a UUID
                let object = object.map_err(|_| SymbolError::Corrupt)?;

                if object.debug_id() != id {
                    continue;
                }

                tracing::trace!(object.format = %object.file_format());

                match func(&object) {
                    Ok(thing) => found = Some(thing),
                    Err(SymbolError::NotFound) => continue,
                    Err(e) => return Err(e),
                }

                // Keep looking if we "only" found a breakpad symbols.
                // We should prefer native symbols if we can get them.
                if object.file_format() != FileFormat::Breakpad {
                    tracing::trace!("non-breakpad object found, stopping");
                    break 'outer;
                }
            }
        }
        found.ok_or(SymbolError::NotFound)
    }
}

#[async_trait]
impl<'a> minidump_processor::SymbolProvider for LocalSymbolProvider<'a> {
    #[tracing::instrument(
        level = "trace",
        skip(self, module, frame),
        fields(module.id, frame.instruction = frame.get_instruction())
    )]
    async fn fill_symbol(
        &self,
        module: &(dyn Module + Sync),
        frame: &mut (dyn FrameSymbolizer + Send),
    ) -> Result<(), FillSymbolError> {
        if !self.symbolicate {
            return Err(FillSymbolError {});
        }

        let id = module.debug_identifier().ok_or(FillSymbolError {})?;
        tracing::Span::current().record("module.id", &tracing::field::display(id));

        let mut symcaches = self.symcaches.write();

        let symcache = symcaches.entry(id).or_insert_with(|| {
            tracing::trace!("symcache needs to be loaded");
            self.load_symbol_info(id)
        });

        let symcache = match symcache {
            Ok(symcache) => symcache,
            Err(e) => {
                tracing::trace!(%e, "symcache could not be loaded");
                return Err(FillSymbolError {});
            }
        };

        tracing::trace!("symcache successfully loaded");

        let instruction = frame.get_instruction();
        let line_info = symcache
            .get()
            .lookup(instruction - module.base_address())
            .map_err(|_| FillSymbolError {})?
            .next()
            .ok_or(FillSymbolError {})?
            .map_err(|_| FillSymbolError {})?;

        frame.set_function(
            line_info.function_name().as_str(),
            line_info.function_address(),
            0,
        );

        frame.set_source_file(line_info.filename(), line_info.line(), 0);

        Ok(())
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, module, walker),
        fields(module.id, frame.instruction = walker.get_instruction())
    )]
    async fn walk_frame(
        &self,
        module: &(dyn Module + Sync),
        walker: &mut (dyn FrameWalker + Send),
    ) -> Option<()> {
        if !self.use_cfi {
            return None;
        }

        let id = module.debug_identifier()?;
        tracing::Span::current().record("module.id", &tracing::field::display(id));

        let mut cfi = self.cfi_files.write();

        let symbol_file = cfi.entry(id).or_insert_with(|| {
            tracing::trace!("cfi needs to be loaded");
            self.load_cfi(id)
        });

        match symbol_file {
            Ok(file) => {
                tracing::trace!("cfi successfully loaded");
                file.walk_frame(module, walker)
            }
            Err(e) => {
                tracing::trace!(%e, "cfi could not be loaded");
                None
            }
        }
    }

    fn stats(&self) -> HashMap<String, SymbolStats> {
        self.cfi_files
            .read()
            .iter()
            .map(|(debug_id, sym)| {
                let stats = SymbolStats {
                    symbol_url: None,
                    loaded_symbols: matches!(sym, Ok(_)),
                    corrupt_symbols: matches!(sym, Err(SymbolError::Corrupt)),
                };

                (debug_id.to_string(), stats)
            })
            .collect()
    }
}

fn symbolize<'a>(
    symcaches: &'a SymCaches<'a>,
    frame: &StackFrame,
    arch: Arch,
    crashing: bool,
) -> Option<Vec<SourceLocation<'a, 'a>>> {
    let module = match &frame.module {
        Some(module) => module,
        None => return None,
    };

    let symcache = match module.debug_identifier().and_then(|id| symcaches.get(&id)) {
        Some(Ok(symcache)) => symcache,
        _ => return None,
    };

    // TODO: Extract and supply signal and IP register
    let return_address = frame.resume_address;
    let caller_address = InstructionInfo::new(arch, return_address)
        .is_crashing_frame(crashing)
        .caller_address();
    let lines = symcache
        .get()
        .lookup(caller_address - module.base_address())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

#[derive(Clone, Copy, Debug)]
struct PrintOptions {
    crashed_only: bool,
    show_modules: bool,
}

struct Report<'a> {
    process_state: ProcessState,
    cfi_files: CfiFiles,
    symcaches: SymCaches<'a>,
    options: PrintOptions,
}

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sys = &self.process_state.system_info;
        writeln!(f, "Operating system: {}", sys.os)?;
        writeln!(
            f,
            "                  {} {}",
            sys.os_version.as_deref().unwrap_or("unknown version"),
            sys.os_build.as_deref().unwrap_or("unknown_build")
        )?;
        writeln!(f,)?;

        writeln!(f, "CPU: {}", sys.cpu)?;
        if let Some(ref cpu_info) = sys.cpu_info {
            writeln!(f, "     {}", cpu_info)?;
        }
        writeln!(f, "     {} CPUs", sys.cpu_count)?;
        writeln!(f,)?;

        if let Some(ref assertion) = self.process_state.assertion {
            writeln!(f, "Assertion:     {}", assertion)?;
        }
        if let Some(crash_reason) = self.process_state.crash_reason {
            writeln!(f, "Crash reason:  {}", crash_reason)?;
        }
        if let Some(crash_address) = self.process_state.crash_address {
            writeln!(f, "Crash address: 0x{:x}", crash_address)?;
        }
        if let Ok(duration) = self.process_state.time.duration_since(UNIX_EPOCH) {
            writeln!(f, "Crash time:    {}", duration.as_secs())?;
        }

        let arch = match sys.cpu {
            minidump::system_info::Cpu::X86 => Arch::X86,
            minidump::system_info::Cpu::X86_64 => Arch::Amd64,
            minidump::system_info::Cpu::Ppc => Arch::Ppc,
            minidump::system_info::Cpu::Ppc64 => Arch::Ppc64,
            minidump::system_info::Cpu::Arm => Arch::Arm,
            minidump::system_info::Cpu::Arm64 => Arch::Arm64,
            minidump::system_info::Cpu::Mips => Arch::Mips,
            minidump::system_info::Cpu::Mips64 => Arch::Mips64,
            _ => Arch::Unknown,
        };

        // for writeing: 8 digits + 0x prefix for 32bit, 16 digits + prefix otherwise
        let address_width = if sys.cpu.pointer_width() == PointerWidth::Bits32 {
            10
        } else {
            18
        };

        for (ti, thread) in self.process_state.threads.iter().enumerate() {
            let crashed = self
                .process_state
                .requesting_thread
                .map_or(false, |i| ti == i);

            if self.options.crashed_only && !crashed {
                continue;
            }

            if crashed {
                writeln!(f, "\nThread {} (crashed)", ti)?;
            } else {
                writeln!(f, "\nThread {}", ti)?;
            }

            let mut index = 0;
            for (fi, frame) in thread.frames.iter().enumerate() {
                if let Some(ref module) = frame.module {
                    if let Some(line_infos) = symbolize(&self.symcaches, frame, arch, fi == 0) {
                        for (i, info) in line_infos.iter().enumerate() {
                            writeln!(
                                f,
                                "{:>3}  {}!{} [{} : {}]",
                                index,
                                module
                                    .debug_file()
                                    .as_deref()
                                    .unwrap_or("<unknown debug file>"),
                                info.function_name()
                                    .try_demangle(DemangleOptions::name_only()),
                                info.filename(),
                                info.line(),
                            )?;

                            if i + 1 < line_infos.len() {
                                writeln!(f, "     Found by: inlined into next frame")?;
                                index += 1;
                            }
                        }
                    } else {
                        writeln!(
                            f,
                            "{:>3}  {} + {:#x}",
                            index,
                            module
                                .debug_file()
                                .as_deref()
                                .unwrap_or("<unknown debug file>"),
                            frame.instruction - module.base_address()
                        )?;
                    }
                } else {
                    writeln!(f, "{:>3}  {:#x}", index, frame.instruction)?;
                }

                let mut newline = true;
                for (name, value) in frame.context.valid_registers() {
                    newline = !newline;
                    write!(f, "     {:>4} = {:#02$x}", name, value, address_width)?;
                    if newline {
                        writeln!(f,)?;
                    }
                }

                if !newline {
                    writeln!(f,)?;
                }

                let trust = match frame.trust {
                    FrameTrust::None => "none",
                    FrameTrust::Scan => "stack scanning",
                    FrameTrust::CfiScan => "call frame info with scanning",
                    FrameTrust::FramePointer => "previous frame's frame pointer",
                    FrameTrust::CallFrameInfo => "call frame info",
                    FrameTrust::PreWalked => "recovered by external stack walker",
                    FrameTrust::Context => "given as instruction pointer in context",
                };

                writeln!(f, "     Found by: {}", trust)?;
                index += 1;
            }
        }

        if self.options.show_modules {
            writeln!(f,)?;
            writeln!(f, "Loaded modules:")?;
            for module in self.process_state.modules.by_addr() {
                write!(
                    f,
                    "{:#018x} - {:#018x}  {}  (",
                    module.base_address(),
                    module.base_address() + module.size() - 1,
                    module.code_file().rsplit('/').next().unwrap(),
                )?;

                let id = module.debug_identifier();

                match id {
                    Some(id) => write!(f, "{}", id.breakpad())?,
                    None => write!(f, "<missing debug identifier>")?,
                };

                match id.and_then(|id| self.symcaches.get(&id)) {
                    Some(Ok(_)) => {}
                    _ => write!(f, "; no symbols")?,
                }

                match id.and_then(|id| self.cfi_files.get(&id)) {
                    Some(Ok(_)) => {}
                    _ => write!(f, "; no CFI")?,
                }

                writeln!(f, ")")?;
            }
        }

        Ok(())
    }
}

async fn execute(matches: &ArgMatches) -> Result<(), Error> {
    let minidump_path = matches.value_of("minidump_file_path").unwrap();
    let symbols_path = matches.value_of("debug_symbols_path").unwrap_or("invalid");

    let symbol_provider = LocalSymbolProvider::new(
        symbols_path.into(),
        matches.is_present("cfi"),
        matches.is_present("symbolize"),
    );

    let minidump = Minidump::read_path(minidump_path)?;
    let process_state = minidump_processor::process_minidump(&minidump, &symbol_provider).await?;

    let options = PrintOptions {
        crashed_only: matches.is_present("only_crash"),
        show_modules: !matches.is_present("no_modules"),
    };

    let (cfi_files, symcaches) = symbol_provider.into_inner();
    print!(
        "{}",
        Report {
            process_state,
            cfi_files,
            symcaches,
            options
        }
    );

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("symbolic-minidump")
        .about("Symbolicates a minidump")
        .arg(
            Arg::new("minidump_file_path")
                .required(true)
                .value_name("minidump")
                .help("Path to the minidump file"),
        )
        .arg(
            Arg::new("debug_symbols_path")
                .value_name("symbols")
                .help("Path to a folder containing debug symbols"),
        )
        .arg(
            Arg::new("cfi")
                .short('c')
                .long("cfi")
                .help("Use CFI while stackwalking"),
        )
        .arg(
            Arg::new("symbolize")
                .short('s')
                .long("symbolize")
                .help("Symbolize frames (file, function and line number)"),
        )
        .arg(
            Arg::new("only_crash")
                .short('o')
                .long("only-crash")
                .help("Only output the crashed thread"),
        )
        .arg(
            Arg::new("no_modules")
                .short('n')
                .long("no-modules")
                .help("Do not output loaded modules"),
        )
        .get_matches();

    match execute(&matches).await {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
