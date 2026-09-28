//! Synthesizes SymCache contents for WASM modules that ship an Emscripten source map instead of
//! DWARF.
//!
//! Emscripten can emit a Source Map v3 file next to a `.wasm` module. The map abuses the source map
//! format: every mapping sits on destination line `0`, and the destination column is a byte offset
//! into the module. That offset is exactly the address SymCaches are keyed by, so a map plus the
//! module it belongs to carries enough information to fill a SymCache.
//!
//! The module is a required input, not an optional one. The map has no notion of functions, so
//! function bounds and function names both come from the `.wasm` binary. Without them a lookup
//! could only ever return a line, and an address just past the last mapping in a function would
//! silently inherit that function's last line.
//!
//! Two guards keep the result from being confidently wrong:
//!
//!  - Mappings outside any function body are dropped. They fall in the padding between bodies,
//!    where no instruction can live.
//!  - Emscripten marks holes inside a function with a one field segment, which decodes to a token
//!    with no source. Those close the preceding range instead of extending it.

use std::borrow::Cow;

use sourcemap::{DecodedMap, Token};
use symbolic_common::{Language, Name, NameMangling, split_path_bytes};
use symbolic_debuginfo::wasm::WasmObject;
use symbolic_debuginfo::{FileInfo, Function, LineInfo};

use crate::SymCacheConverter;

/// An error that happened while reading a WASM source map.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WasmSourceMapError {
    /// The source map could not be parsed.
    #[error("failed to parse the source map")]
    Parse(#[source] sourcemap::Error),

    /// The source map refers to a name that it does not declare.
    ///
    /// Emscripten 4.0.2x briefly emitted five field segments while leaving `names` empty. The
    /// decoder rejects the whole map in that case, so the only fix is on the producing side.
    #[error("source map references undefined name #{0}; rebuild with a newer emscripten")]
    BadNameReference(u32),

    /// The source map is not a plain map.
    #[error("expected a plain source map, found an index or Hermes map")]
    UnsupportedFormat,

    /// The source map maps more than one destination line, so it is not a WASM map.
    #[error("source map maps destination line {0}, expected a single line at 0")]
    NotWasm(u32),
}

/// Counters describing how well a source map lined up with the module it was joined against.
///
/// Callers report these so that a map built against a different module shows up as a drop in
/// coverage rather than as silently wrong symbolication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WasmSourceMapStats {
    /// Function bodies in the module.
    pub functions_total: u32,
    /// Function bodies that got at least one line record.
    pub functions_mapped: u32,
    /// Mappings that turned into a line record.
    pub tokens_used: u32,
    /// Mappings that fell outside every function body and were dropped.
    pub tokens_out_of_range: u32,
    /// One field terminator segments. These close a line range and never become a line themselves.
    pub tokens_unmapped: u32,
}

/// Fills `converter` from `object` and its Emscripten source map.
///
/// The map is joined against the module's function bodies. Every body ends up in the cache, even
/// one the map says nothing about: knowing the function but not the line is useful, and it stops
/// the next lookup from reaching back into the previous function.
pub fn process_wasm_sourcemap(
    converter: &mut SymCacheConverter<'_>,
    object: &WasmObject<'_>,
    sourcemap: &[u8],
) -> Result<WasmSourceMapStats, WasmSourceMapError> {
    let sourcemap = match sourcemap::decode_slice(sourcemap) {
        Ok(DecodedMap::Regular(sourcemap)) => sourcemap,
        Ok(_) => return Err(WasmSourceMapError::UnsupportedFormat),
        Err(sourcemap::Error::BadNameReference(id)) => {
            return Err(WasmSourceMapError::BadNameReference(id));
        }
        Err(error) => return Err(WasmSourceMapError::Parse(error)),
    };

    let mut tokens = Vec::with_capacity(sourcemap.get_token_count() as usize);
    for token in sourcemap.tokens() {
        let line = token.get_dst_line();
        if line != 0 {
            return Err(WasmSourceMapError::NotWasm(line));
        }
        tokens.push(token);
    }
    // `tokens()` yields in mapping order, which for a single destination line is column order.

    converter.set_arch(object.arch());
    converter.set_debug_id(object.debug_id());

    let mut stats = WasmSourceMapStats::default();
    let mut cursor = 0;

    for body in object.function_bodies() {
        stats.functions_total += 1;

        let start = body.address;
        let end = start + body.size;

        while cursor < tokens.len() && (tokens[cursor].get_dst_col() as u64) < start {
            stats.tokens_out_of_range += 1;
            cursor += 1;
        }

        let first = cursor;
        while cursor < tokens.len() && (tokens[cursor].get_dst_col() as u64) < end {
            cursor += 1;
        }

        let lines = line_records(&tokens[first..cursor], start, end, &mut stats);
        if lines.iter().any(|line| line.line != 0) {
            stats.functions_mapped += 1;
        }

        let name = match body.name {
            Some(name) => Cow::Borrowed(name),
            // The name section is optional and release builds often drop it. The synthesized name
            // matches what browsers show for the same function.
            None => Cow::Owned(format!("wasm-function[{}]", body.index)),
        };

        converter.process_symbolic_function(&Function {
            address: start,
            size: body.size,
            name: Name::new(name, NameMangling::Unknown, Language::Unknown),
            compilation_dir: &[],
            lines,
            inlinees: Vec::new(),
            inline: false,
            variables: Vec::new(),
        });
    }

    stats.tokens_out_of_range += (tokens.len() - cursor) as u32;

    Ok(stats)
}

/// Turns the mappings covering one function body into a gapless list of line records.
///
/// Each record runs until the next mapping, or until the end of the body. Ranges the map says
/// nothing about still get a record, with no file and no line, so that a lookup there reports the
/// function and stops.
fn line_records<'data>(
    tokens: &[Token<'data>],
    start: u64,
    end: u64,
    stats: &mut WasmSourceMapStats,
) -> Vec<LineInfo<'data>> {
    let mut records: Vec<LineInfo<'data>> = Vec::with_capacity(tokens.len() + 1);

    if tokens
        .first()
        .is_none_or(|token| token.get_dst_col() as u64 > start)
    {
        records.push(unmapped_record(start));
    }

    for token in tokens {
        let address = token.get_dst_col() as u64;
        let record = if token.has_source() {
            stats.tokens_used += 1;
            let path = token.get_source().unwrap_or_default().as_bytes();
            let (dir, name) = split_path_bytes(path);
            LineInfo {
                address,
                size: None,
                file: FileInfo::new(Cow::Borrowed(dir.unwrap_or_default()), Cow::Borrowed(name)),
                // Source maps count lines from zero, SymCaches count from one.
                line: token.get_src_line() as u64 + 1,
            }
        } else {
            stats.tokens_unmapped += 1;
            unmapped_record(address)
        };

        // Several segments can share an offset. The last one wins, which matches how a source map
        // consumer resolves that offset.
        match records.last_mut() {
            Some(last) if last.address == address => *last = record,
            _ => records.push(record),
        }
    }

    for index in 0..records.len() {
        let next = records.get(index + 1).map_or(end, |record| record.address);
        records[index].size = Some(next - records[index].address);
    }

    records
}

/// A range that belongs to a function but has no known source location.
///
/// The writer recognizes a record with no line and no file and keeps the function while dropping
/// the source location.
fn unmapped_record<'data>(address: u64) -> LineInfo<'data> {
    LineInfo {
        address,
        size: None,
        file: FileInfo::new(Cow::Borrowed(&[]), Cow::Borrowed(&[])),
        line: 0,
    }
}
