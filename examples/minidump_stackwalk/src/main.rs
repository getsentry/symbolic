use core::fmt;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use minidump::system_info::PointerWidth;
use minidump::{Minidump, Module};
use minidump_processor::{
    FillSymbolError, FrameSymbolizer, FrameTrust, FrameWalker, ProcessState, StackFrame,
    SymbolFile, SymbolStats,
};
use thiserror::Error;
use walkdir::WalkDir;

use symbolic::cfi::CfiCache;
use symbolic::common::{Arch, ByteView, DebugId, InstructionInfo, SelfCell};
use symbolic::debuginfo::{Archive, FileFormat};
use symbolic::demangle::{Demangle, DemangleOptions};
use symbolic::symcache::{SourceLocation, SymCache, SymCacheConverter};

type CfiFiles = BTreeMap<DebugId, Result<SymbolFile, SymbolError>>;
type SymCaches<'a> = BTreeMap<DebugId, Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError>>;
type ObjectDatabase = BTreeMap<DebugId, Vec<ObjectMetadata>>;
type Error = Box<dyn std::error::Error>;

#[derive(Debug, Clone, Copy, Error)]
enum SymbolError {
    #[error("not found")]
    NotFound,
    #[error("corrupt debug file")]
    Corrupt,
}

/// Metadata about an object in the filesystem.
#[derive(Debug, Clone)]
struct ObjectMetadata {
    /// The object's path.
    path: PathBuf,
    /// The object's index in its archive.
    index_in_archive: usize,
    /// Whether the object has unwind info.
    has_unwind_info: bool,
    /// Whether the object has symbol info.
    has_symbol_info: bool,
}

/// A SymbolProvider that recursively searches a given path for symbol files.
struct LocalSymbolProvider<'a> {
    object_files: ObjectDatabase,
    cfi_files: Mutex<CfiFiles>,
    symcaches: Mutex<SymCaches<'a>>,
    use_cfi: bool,
    symbolicate: bool,
}

impl<'a> LocalSymbolProvider<'a> {
    /// Constructs a `LocalSymbolProvider` that will look for symbol files under the given path.
    fn new<P: AsRef<Path>>(path: Option<P>, use_cfi: bool, symbolicate: bool) -> Self {
        Self {
            object_files: path.map_or(Default::default(), |path| {
                Self::create_object_database(path)
            }),
            cfi_files: Mutex::new(BTreeMap::default()),
            symcaches: Mutex::new(SymCaches::default()),
            use_cfi,
            symbolicate,
        }
    }

    /// Accumulates a database of objects found under the given path.
    ///
    /// The objects are saved in a map from `DebugId`s to vectors of
    /// `[ObjectMetadata]`. The latter contains the following information:
    /// * the object's path
    /// * the object's index in its archive
    /// * whether the object has unwind info
    /// * whether the object has symbol info
    #[tracing::instrument(skip_all, fields(path = ?path.as_ref()))]
    fn create_object_database(path: impl AsRef<Path>) -> ObjectDatabase {
        let mut object_db = ObjectDatabase::new();
        for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
            // Folders will be recursed into automatically
            if !entry.metadata().map_or(false, |md| md.is_file()) {
                continue;
            }

            // Try to parse a potential object file. If this is not possible, then
            // we're not dealing with an object file, thus silently skipping it
            let buffer = match ByteView::open(entry.path()) {
                Ok(buffer) => buffer,
                Err(_e) => continue,
            };

            let archive = match Archive::parse(&buffer) {
                Ok(archive) => archive,
                Err(_e) => continue,
            };

            for (idx, object) in archive.objects().enumerate() {
                // Fail for invalid matching objects but silently skip objects
                // without a UUID
                let object = match object {
                    Ok(object) => object,
                    Err(_e) => continue,
                };

                let id = object.debug_id();

                let object_list = object_db.entry(id).or_insert_with(Vec::new);
                tracing::trace!(
                    object.path = ?entry.path(),
                    object.has_unwind_info = object.has_unwind_info(),
                    object.has_symbol_info = object.has_debug_info(),
                    "object found"
                );
                object_list.push(ObjectMetadata {
                    path: entry.path().into(),
                    index_in_archive: idx,
                    has_unwind_info: object.has_unwind_info(),
                    has_symbol_info: object.has_debug_info(),
                })
            }
        }

        object_db
    }
    /// Consumes this `LocalSymbolProvider` and returns its collections of cfi and debug files.
    fn into_inner(self) -> (CfiFiles, SymCaches<'a>) {
        (
            self.cfi_files.into_inner().unwrap(),
            self.symcaches.into_inner().unwrap(),
        )
    }

    /// Attempt to load CFI for the given debug id.
    ///
    /// The id is looked up in the symbol provider's `object_files` database.
    /// Objects which have unwind information are then tried in order.
    #[tracing::instrument(skip_all, fields(id = %id))]
    fn load_cfi(&self, id: DebugId) -> Result<SymbolFile, SymbolError> {
        let object_list = self.object_files.get(&id).ok_or(SymbolError::NotFound)?;
        let mut found = None;
        for object_meta in object_list.iter().filter(|object| object.has_unwind_info) {
            tracing::trace!(path = ?object_meta.path, "trying object file");
            let buffer = ByteView::open(&object_meta.path).unwrap();
            let archive = Archive::parse(&buffer).unwrap();

            let object = archive
                .objects()
                .nth(object_meta.index_in_archive)
                .unwrap()
                .unwrap();
            let cfi_cache = match CfiCache::from_object(&object) {
                Ok(cficache) => cficache,
                Err(_e) => continue,
            };

            if cfi_cache.as_slice().is_empty() {
                continue;
            }

            match SymbolFile::from_bytes(cfi_cache.as_slice()) {
                Ok(symbol_file) => {
                    tracing::trace!("successfully parsed cficache");
                    found = Some(symbol_file);
                }
                Err(_e) => continue,
            }

            if object.file_format() != FileFormat::Breakpad {
                break;
            }
        }

        found.ok_or(SymbolError::NotFound)
    }

    /// Attempt to load CFI for the given debug id.
    ///
    /// The id is looked up in the symbol provider's `object_files` database.
    /// Objects which have symbol information are then tried in order.
    #[tracing::instrument(skip_all, fields(id = %id))]
    fn load_symbol_info(
        &self,
        id: DebugId,
    ) -> Result<SelfCell<ByteView<'a>, SymCache<'a>>, SymbolError> {
        let object_list = self.object_files.get(&id).ok_or(SymbolError::NotFound)?;
        let mut found = None;
        for object_meta in object_list.iter().filter(|object| object.has_symbol_info) {
            tracing::trace!(path = ?object_meta.path, "trying object file");
            let buffer = ByteView::open(&object_meta.path).unwrap();
            let archive = Archive::parse(&buffer).unwrap();

            let object = archive
                .objects()
                .nth(object_meta.index_in_archive)
                .unwrap()
                .unwrap();

            let mut buffer = Vec::new();
            let mut converter = SymCacheConverter::new();
            if let Err(e) = converter.process_object(&object) {
                tracing::error!(error = %e);
                return Err(SymbolError::Corrupt);
            }
            if let Err(e) = converter.serialize(&mut Cursor::new(&mut buffer)) {
                tracing::error!(error = %e);
                return Err(SymbolError::Corrupt);
            }

            match SelfCell::try_new(ByteView::from_vec(buffer), |ptr| {
                SymCache::parse(unsafe { &*ptr })
            }) {
                Ok(symcache) => {
                    tracing::trace!("successfully parsed symcache");
                    found = Some(symcache);
                }
                Err(_e) => continue,
            }

            if object.file_format() != FileFormat::Breakpad {
                break;
            }
        }

        found.ok_or(SymbolError::NotFound)
    }
}

#[async_trait]
impl<'a> minidump_processor::SymbolProvider for LocalSymbolProvider<'a> {
    #[tracing::instrument(
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

        let mut symcaches = self.symcaches.lock().unwrap();

        let symcache = symcaches.entry(id).or_insert_with(|| {
            tracing::info!("loading symcache for the first time");
            self.load_symbol_info(id)
        });

        let symcache = match symcache {
            Ok(symcache) => symcache,
            Err(e) => {
                tracing::warn!(error = %e, "symcache could not be loaded");
                return Err(FillSymbolError {});
            }
        };

        tracing::info!("symcache successfully loaded");

        let instruction = frame.get_instruction();
        let source_location = symcache
            .get()
            .lookup(instruction - module.base_address())
            .last()
            .ok_or(FillSymbolError {})?;

        frame.set_function(
            source_location.function().name(),
            source_location.function().entry_pc() as u64,
            0,
        );

        if let Some(file) = source_location.file() {
            frame.set_source_file(&file.full_path(), source_location.line(), 0);
        }

        Ok(())
    }

    #[tracing::instrument(
        skip(self, module, walker),
        fields(module.id, frame.instruction = walker.get_instruction())
    )]
    async fn walk_frame(
        &self,
        module: &(dyn Module + Sync),
        walker: &mut (dyn FrameWalker + Send),
    ) -> Option<()> {
        tracing::info!("walk_frame called");
        if !self.use_cfi {
            return None;
        }

        let id = module.debug_identifier()?;
        tracing::Span::current().record("module.id", &tracing::field::display(id));

        let mut cfi = self.cfi_files.lock().unwrap();

        let symbol_file = cfi.entry(id).or_insert_with(|| {
            tracing::info!("loading cficache for the first time");
            self.load_cfi(id)
        });

        match symbol_file {
            Ok(file) => {
                tracing::info!("cfi successfully loaded");
                file.walk_frame(module, walker)
            }
            Err(e) => {
                tracing::warn!(error = %e, "cfi could not be loaded");
                None
            }
        }
    }

    fn stats(&self) -> HashMap<String, SymbolStats> {
        self.cfi_files
            .lock()
            .unwrap()
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

    async fn get_file_path(
        &self,
        _module: &(dyn Module + Sync),
        _kind: minidump_processor::FileKind,
    ) -> Result<PathBuf, minidump_processor::FileError> {
        Err(minidump_processor::FileError::NotFound)
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
                                info.function()
                                    .name_for_demangling()
                                    .try_demangle(DemangleOptions::name_only()),
                                info.file()
                                    .map(|file| file.full_path())
                                    .unwrap_or_else(|| "<unknown source file>".into()),
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
    let minidump_path = matches.get_one::<PathBuf>("minidump_file_path").unwrap();
    let symbols_path = matches.get_one::<PathBuf>("debug_symbols_path");

    let symbol_provider = LocalSymbolProvider::new(
        symbols_path,
        *matches.get_one("cfi").unwrap(),
        *matches.get_one("symbolize").unwrap(),
    );

    let minidump = Minidump::read_path(minidump_path)?;
    let process_state = minidump_processor::process_minidump(&minidump, &symbol_provider).await?;

    let options = PrintOptions {
        crashed_only: *matches.get_one("only_crash").unwrap(),
        show_modules: *matches.get_one("show_modules").unwrap(),
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
                .value_parser(value_parser!(PathBuf))
                .help("Path to the minidump file"),
        )
        .arg(
            Arg::new("debug_symbols_path")
                .value_name("symbols")
                .value_parser(value_parser!(PathBuf))
                .help("Path to a folder containing debug symbols"),
        )
        .arg(
            Arg::new("cfi")
                .short('c')
                .long("cfi")
                .action(ArgAction::SetTrue)
                .help("Use CFI while stackwalking"),
        )
        .arg(
            Arg::new("symbolize")
                .short('s')
                .long("symbolize")
                .action(ArgAction::SetTrue)
                .help("Symbolize frames (file, function and line number)"),
        )
        .arg(
            Arg::new("only_crash")
                .short('o')
                .long("only-crash")
                .action(ArgAction::SetTrue)
                .help("Only output the crashed thread"),
        )
        .arg(
            Arg::new("show_modules")
                .short('n')
                .long("no-modules")
                .action(ArgAction::SetFalse)
                .help("Do not output loaded modules"),
        )
        .get_matches();

    match execute(&matches).await {
        Ok(()) => (),
        Err(e) => println!("Error: {}", e),
    };
}
