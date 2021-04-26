use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use walkdir::WalkDir;

use symbolic_common::ByteView;
use symbolic_debuginfo::{Archive, FileFormat, Object};
use symbolic_minidump::cfi::CfiCache;
use symbolic_minidump::processor::{CodeModuleId, FrameInfoMap, ProcessState};

type Error = Box<dyn std::error::Error>;

/// Benchmark the minidump stackwalker on a third-party minidump file.
///
/// This benchmark works similarly to the `minidump_stackwalk` binary. It is designed to
/// be used with third-party minidumps that cannot be added as benchmarks. To run it,
/// first replace `minidump_path` with the path to a minidump file and `symbols_path`
/// with the path to a symbol file (or a directory containing symbol files) and then
/// do `cargo bench from_minidump_external`.
///
/// Note: The benchmark only measures the `from_minidump` call; CFI and symbol information
/// is collected beforehand.
pub fn minidump_external_benchmark(c: &mut Criterion) {
    let minidump_path = "/path/to/minidump";
    let symbols_path = "/path/to/symbol/files";

    // Initially process without CFI
    let buffer = ByteView::open(&minidump_path).unwrap();
    let state = ProcessState::from_minidump(&buffer, None).unwrap();

    // Obtain Call Frame Information
    let frame_info = prepare_cfi(&symbols_path, &state).unwrap();

    let mut group = c.benchmark_group("External Minidump");

    group.bench_with_input(
        BenchmarkId::new("from_minidump_breakpad", "external files"),
        &(&buffer, &frame_info),
        |b, (buffer, frame_info)| {
            b.iter(|| ProcessState::from_minidump_breakpad(buffer, Some(frame_info)))
        },
    );

    group.bench_with_input(
        BenchmarkId::new("from_minidump", "external files"),
        &(&buffer, &frame_info),
        |b, (buffer, frame_info)| b.iter(|| ProcessState::from_minidump(buffer, Some(frame_info))),
    );

    group.finish();
}

criterion_group!(benches, minidump_external_benchmark);
criterion_main!(benches);

fn collect_referenced_objects<P, F, T>(
    path: P,
    state: &ProcessState,
    mut func: F,
) -> Result<BTreeMap<CodeModuleId, T>, Error>
where
    P: AsRef<Path>,
    F: FnMut(Object, &Path) -> Result<Option<T>, Error>,
{
    let search_ids: HashSet<_> = state
        .modules()
        .iter()
        .filter_map(|module| module.id())
        .collect();

    let mut collected = BTreeMap::new();
    let mut final_ids = HashSet::new();
    for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
        // Folders will be recursed into automatically
        if !entry.metadata()?.is_file() {
            continue;
        }

        // Try to parse a potential object file. If this is not possible, then
        // we're not dealing with an object file, thus silently skipping it
        let buffer = ByteView::open(entry.path())?;
        let archive = match Archive::parse(&buffer) {
            Ok(archive) => archive,
            Err(_) => continue,
        };

        for object in archive.objects() {
            // Fail for invalid matching objects but silently skip objects
            // without a UUID
            let object = object?;
            let id = CodeModuleId::from(object.debug_id());

            // Make sure we haven't converted this object already
            if !search_ids.contains(&id) || final_ids.contains(&id) {
                continue;
            }

            let format = object.file_format();
            if let Some(t) = func(object, entry.path())? {
                collected.insert(id, t);

                // Keep looking if we "only" found a breakpad symbols.
                // We should prefer native symbols if we can get them.
                if format != FileFormat::Breakpad {
                    final_ids.insert(id);
                }
            }
        }
    }

    Ok(collected)
}

fn prepare_cfi<P>(path: P, state: &ProcessState) -> Result<FrameInfoMap<'static>, Error>
where
    P: AsRef<Path>,
{
    collect_referenced_objects(path, state, |object, path| {
        // Silently skip all debug symbols without CFI
        if !object.has_unwind_info() {
            return Ok(None);
        }

        // Silently skip conversion errors
        Ok(match CfiCache::from_object(&object) {
            Ok(cficache) => Some(cficache),
            Err(e) => {
                eprintln!("[cfi] {}: {}", path.display(), e);
                None
            }
        })
    })
}
