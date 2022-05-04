use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command};
use minidump::{Minidump, Module};
use minidump_processor::{
    FillSymbolError, FrameSymbolizer, FrameTrust, FrameWalker, ProcessState, StackFrame,
    SymbolFile, SymbolProvider, SymbolStats,
};
use parking_lot::RwLock;
use walkdir::WalkDir;

use symbolic::common::{Arch, ByteView, DebugId, InstructionInfo, SelfCell};
use symbolic::debuginfo::{Archive, FileFormat};
use symbolic::demangle::{Demangle, DemangleOptions};
use symbolic::minidump::cfi::CfiCache;
use symbolic::symcache::{Error as SymCacheError, SourceLocation, SymCache, SymCacheConverter};

type SymCaches<'a> = BTreeMap<DebugId, Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError>>;
type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, Copy)]
enum SymbolError {
    NotFound,
    Corrupt,
    Other,
}

struct LocalSymbolProvider<'a> {
    symbols_path: PathBuf,
    cfi: RwLock<BTreeMap<DebugId, Result<SymbolFile, SymbolError>>>,
    symcaches: RwLock<SymCaches<'a>>,
    use_cfi: bool,
    symbolize: bool,
}

impl<'a> LocalSymbolProvider<'a> {
    /// Load the CFI information from the cache.
    ///
    /// This reads the CFI caches from disk and returns them in a format suitable for the
    /// processor to stackwalk.
    #[tracing::instrument]
    pub fn new(path: PathBuf, use_cfi: bool, symbolize: bool) -> Self {
        Self {
            symbols_path: path,
            cfi: RwLock::new(BTreeMap::default()),
            symcaches: RwLock::new(SymCaches::default()),
            use_cfi,
            symbolize,
        }
    }

    #[tracing::instrument(skip(self))]
    fn load_cfi(&self, id: DebugId) -> Result<SymbolFile, SymbolError> {
        if !self.use_cfi {
            return Err(SymbolError::Other);
        }

        let mut found = None;

        for entry in WalkDir::new(&self.symbols_path)
            .into_iter()
            .filter_map(Result::ok)
        {
            tracing::trace!(path = ?entry.path());
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

                if !object.has_unwind_info() {
                    continue;
                }

                let cfi_cache = match CfiCache::from_object(&object) {
                    Ok(cficache) => cficache,
                    Err(e) => {
                        eprintln!("[cfi] {}: {}", self.symbols_path.display(), e);
                        continue;
                    }
                };

                if cfi_cache.as_slice().is_empty() {
                    continue;
                }

                let symbol_file = match SymbolFile::from_bytes(cfi_cache.as_slice()) {
                    Ok(symbol_file) => symbol_file,
                    Err(_e) => {
                        //let stderr: &dyn std::error::Error = &e;
                        //tracing::error!(stderr, "Error while processing cficache");
                        continue;
                    }
                };

                found = Some(symbol_file);

                // Keep looking if we "only" found a breakpad symbols.
                // We should prefer native symbols if we can get them.
                if object.file_format() != FileFormat::Breakpad {
                    break;
                }
            }
        }
        found.ok_or(SymbolError::NotFound)
    }

    fn load_symcache(
        &self,
        id: DebugId,
    ) -> Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError> {
        if !self.symbolize {
            return Err(SymbolError::Other);
        }

        let mut found = None;

        for entry in WalkDir::new(&self.symbols_path)
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

                // Silently skip all incompatible debug symbols
                if !object.has_debug_info() {
                    continue;
                }

                let mut buffer = Vec::new();
                if let Err(e) = SymCacheWriter::write_object(&object, Cursor::new(&mut buffer)) {
                    eprintln!("[sym] {}: {}", self.symbols_path.display(), e);
                    continue;
                }

                // Silently skip conversion errors
                let symcache = match SelfCell::try_new(ByteView::from_vec(buffer), |ptr| {
                    SymCache::parse(unsafe { &*ptr })
                }) {
                    Ok(symcache) => symcache,
                    Err(_) => continue,
                };

                found = Some(symcache);

                // Keep looking if we "only" found a breakpad symbols.
                // We should prefer native symbols if we can get them.
                if object.file_format() != FileFormat::Breakpad {
                    break;
                }
            }
        }
        found.ok_or(SymbolError::NotFound)
    }
}

#[async_trait]
impl<'a> minidump_processor::SymbolProvider for LocalSymbolProvider<'a> {
    #[tracing::instrument(skip(self, module, frame), fields(module.id = ?module.debug_identifier(), frame.instruction = frame.get_instruction()))]
    async fn fill_symbol(
        &self,
        module: &(dyn Module + Sync),
        frame: &mut (dyn FrameSymbolizer + Send),
    ) -> Result<(), FillSymbolError> {
        let id = module.debug_identifier().ok_or(FillSymbolError {})?;

        let mut symcaches = self.symcaches.write();

        let symcache = symcaches.entry(id).or_insert_with(|| {
            tracing::debug!("symcache needs to be loaded");
            self.load_symcache(id)
        });

        let symcache = match symcache {
            Ok(symcache) => symcache,
            Err(e) => {
                tracing::debug!(?e, "symcache could not be loaded");
                return Err(FillSymbolError {});
            }
        };

        tracing::debug!("symcache successfully loaded");

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

    #[tracing::instrument(skip(self, module, walker), fields(module.id = ?module.debug_identifier()))]
    async fn walk_frame(
        &self,
        module: &(dyn Module + Sync),
        walker: &mut (dyn FrameWalker + Send),
    ) -> Option<()> {
        let id = module.debug_identifier()?;

        let mut cfi = self.cfi.write();

        let symbol_file = cfi.entry(id).or_insert_with(|| {
            tracing::debug!("cfi needs to be loaded");
            self.load_cfi(id)
        });

        match symbol_file {
            Ok(file) => {
                tracing::debug!("cfi successfully loaded");
                file.walk_frame(module, walker)
            }
            Err(e) => {
                tracing::debug!(?e, "cfi could not be loaded");
                None
            }
        }
    }

    fn stats(&self) -> HashMap<String, SymbolStats> {
        self.cfi
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
) -> Result<Option<Vec<SourceLocation<'a, 'a>>>, SymCacheError> {
    let module = match frame.module() {
        Some(module) => module,
        None => return Ok(None),
    };

    let symcache = match module.debug_identifier().and_then(|id| symcaches.get(&id)) {
        Some(Ok(symcache)) => symcache,
        _ => return Ok(None),
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
        Ok(None)
    } else {
        Ok(Some(lines))
    }
}

#[derive(Clone, Copy, Debug)]
struct PrintOptions {
    crashed_only: bool,
    show_modules: bool,
    show_symbol_stats: bool,
}

#[allow(deprecated)]
fn print_state(
    state: &ProcessState,
    symbol_provider: &LocalSymbolProvider,
    options: PrintOptions,
) -> Result<(), Error> {
    let sys = &state.system_info;
    println!("Operating system: {}", sys.os);
    println!(
        "                  {} {}",
        sys.os_version.as_deref().unwrap_or("unknown version"),
        sys.os_build.as_deref().unwrap_or("unknown_build")
    );
    println!();

    println!("CPU: {}", sys.cpu);
    if let Some(ref cpu_info) = sys.cpu_info {
        println!("     {}", cpu_info);
    }
    println!("     {} CPUs", sys.cpu_count);
    println!();

    if let Some(ref assertion) = state.assertion {
        println!("Assertion:     {}", assertion);
    }
    if let Some(crash_reason) = state.crash_reason {
        println!("Crash reason:  {}", crash_reason);
    }
    if let Some(crash_address) = state.crash_address {
        println!("Crash address: 0x{:x}", crash_address);
    }
    if let Ok(duration) = state.time.duration_since(UNIX_EPOCH) {
        println!("Crash time:    {}", duration.as_secs());
    }

    let arch = match state.system_info.cpu {
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

    for (ti, thread) in state.threads.iter().enumerate() {
        let crashed = state.requesting_thread.map_or(false, |i| ti == i);

        if options.crashed_only && !crashed {
            continue;
        }

        if crashed {
            println!("\nThread {} (crashed)", ti);
        } else {
            println!("\nThread {}", ti);
        }

        let mut index = 0;
        for (fi, frame) in thread.frames.iter().enumerate() {
            if let Some(ref module) = frame.module {
                if let Some(line_infos) =
                    symbolize(&symbol_provider.symcaches.read(), frame, arch, fi == 0)?
                {
                    for (i, info) in line_infos.iter().enumerate() {
                        println!(
                            "{:>3}  {}!{} [{} : {}]",
                            index,
                            module.debug_file(),
                            info.function()
                                .name_for_demangling()
                                .try_demangle(DemangleOptions::name_only()),
                            info.file()
                                .map(|file| file.path_name())
                                .unwrap_or("<unknown file>"),
                            info.line(),
                        );

                        if i + 1 < line_infos.len() {
                            println!("     Found by: inlined into next frame");
                            index += 1;
                        }
                    }
                } else {
                    println!(
                        "{:>3}  {} + 0x{:x}",
                        index,
                        module
                            .debug_file()
                            .as_deref()
                            .unwrap_or("unknown debug file"),
                        frame.instruction - module.base_address()
                    );
                }
            } else {
                println!("{:>3}  {:#x}", index, frame.instruction);
            }

            let mut newline = true;
            for (name, value) in frame.context.valid_registers() {
                newline = !newline;
                print!("     {:>4} = {:#x}", name, value);
                if newline {
                    println!();
                }
            }

            if !newline {
                println!();
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

            println!("     Found by: {}", trust);
            index += 1;
        }
    }

    if options.show_modules {
        println!();
        println!("Loaded modules:");
        for module in state.modules.iter() {
            print!(
                "0x{:x} - 0x{:x}  {}  (",
                module.base_address(),
                module.base_address() + module.size() - 1,
                module.code_file().rsplit('/').next().unwrap(),
            );

            let id = module.debug_identifier();

            match id {
                Some(id) => print!("{}", id),
                None => print!("<missing debug identifier>"),
            };

            if !id.map_or(false, |id| {
                symbol_provider.symcaches.read().contains_key(&id)
            }) {
                print!("; no symbols");
            }

            if !id.map_or(false, |id| symbol_provider.cfi.read().contains_key(&id)) {
                print!("; no CFI");
            }

            println!(")");
        }
    }

    if options.show_symbol_stats {
        println!("{:#?}", symbol_provider.stats());
    }

    Ok(())
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
        show_symbol_stats: matches.is_present("symbol_stats"),
    };

    print_state(&process_state, &symbol_provider, options)?;

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
        .arg(
            Arg::new("symbol_stats")
                .long("symbol-stats")
                .help("Print symbol stats"),
        )
        .get_matches();

    match execute(&matches).await {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
