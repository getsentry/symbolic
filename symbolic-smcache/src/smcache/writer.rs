use std::collections::HashMap;
use std::io::Write;

use sourcemap::DecodedMap;
use zerocopy::AsBytes;

use crate::scope_index::{ScopeIndex, ScopeIndexError, ScopeLookupResult};
use crate::source::{SourceContext, SourceContextError};
use crate::{extract_scope_names, NameResolver, SourcePosition};

use super::raw;
use raw::{ANONYMOUS_SCOPE_SENTINEL, GLOBAL_SCOPE_SENTINEL, NO_FILE_SENTINEL};

/// A structure that allows quick resolution of minified [`raw::MinifiedSourcePosition`]s
/// to the original [`raw::OriginalSourceLocation`] it maps to.
pub struct SmCacheWriter {
    string_bytes: Vec<u8>,
    files: Vec<raw::File>,
    line_offsets: Vec<raw::LineOffset>,
    mappings: Vec<(raw::MinifiedSourcePosition, raw::OriginalSourceLocation)>,
}

impl SmCacheWriter {
    /// Constructs a new Cache from a minified source file and its corresponding SourceMap.
    #[tracing::instrument(level = "trace", name = "SmCacheWriter::new", skip_all)]
    pub fn new(source: &str, sourcemap: &str) -> Result<Self, SmCacheWriterError> {
        // TODO: we could sprinkle a few `tracing` spans around this function to
        // figure out which step is expensive and worth optimizing:
        //
        // - sourcemap parsing/flattening
        // - extracting scopes
        // - resolving scopes to original names
        // - converting the indexed scopes into line/col
        // - actually mapping all the tokens and collecting them into the cache
        //
        // what _might_ be expensive is resolving original scope names and
        // converting the `scope_index`, as the offset/position conversion is
        // potentially an `O(n)` operation due to UTF-16 :-(
        // so micro-optimizing that further _might_ be worth it.

        let sm = tracing::trace_span!("decode sourcemap").in_scope(
            || -> Result<DecodedMap, SmCacheWriterError> {
                let sm = sourcemap::decode_slice(sourcemap.as_bytes())
                    .map_err(SmCacheErrorInner::SourceMap)?;
                // flatten the `SourceMapIndex`, as we want to iterate tokens
                Ok(match sm {
                    DecodedMap::Regular(sm) => DecodedMap::Regular(sm),
                    DecodedMap::Index(smi) => {
                        DecodedMap::Regular(smi.flatten().map_err(SmCacheErrorInner::SourceMap)?)
                    }
                    DecodedMap::Hermes(smh) => DecodedMap::Hermes(smh),
                })
            },
        )?;

        let tokens = match &sm {
            DecodedMap::Regular(sm) => sm.tokens(),
            DecodedMap::Hermes(smh) => smh.tokens(),
            DecodedMap::Index(_smi) => unreachable!(),
        };

        let scopes = extract_scope_names(source);

        // resolve scopes to original names
        let ctx = SourceContext::new(source).map_err(SmCacheErrorInner::SourceContext)?;
        let resolver = NameResolver::new(&ctx, &sm);
        let scopes: Vec<_> = tracing::trace_span!("resolve original names").in_scope(|| {
            scopes
                .into_iter()
                .map(|(range, name)| {
                    let name = name
                        .map(|n| resolver.resolve_name(&n))
                        .filter(|s| !s.is_empty());
                    (range, name)
                })
                .collect()
        });

        // convert our offset index to a source position index
        let scope_index = ScopeIndex::new(scopes).map_err(SmCacheErrorInner::ScopeIndex)?;
        let scope_index: Vec<_> = tracing::trace_span!("scope index").in_scope(|| {
            scope_index
                .iter()
                .filter_map(|(offset, result)| {
                    let pos = ctx.offset_to_position(offset);
                    pos.map(|pos| (pos, result))
                })
                .collect()
        });
        let lookup_scope = |sp: &SourcePosition| {
            let idx = match scope_index.binary_search_by_key(&sp, |idx| &idx.0) {
                Ok(idx) => idx,
                Err(0) => 0,
                Err(idx) => idx - 1,
            };
            match scope_index.get(idx) {
                Some(r) => r.1,
                None => ScopeLookupResult::Unknown,
            }
        };

        let orig_files = match &sm {
            DecodedMap::Regular(sm) => sm.sources().zip(sm.source_contents()),
            DecodedMap::Hermes(smh) => smh.sources().zip(smh.source_contents()),
            DecodedMap::Index(_smi) => unreachable!(),
        }
        .map(|(name, source)| (name, source.unwrap_or_default()));

        let mut string_bytes = Vec::new();
        let mut strings = HashMap::new();
        let mut mappings = Vec::new();

        let mut line_offsets = vec![];
        let mut files = vec![];
        tracing::trace_span!("extract original files").in_scope(|| {
            for (name, source) in orig_files {
                let name_offset = Self::insert_string(&mut string_bytes, &mut strings, name);
                let source_offset = Self::insert_string(&mut string_bytes, &mut strings, source);
                let line_offsets_start = line_offsets.len() as u32;
                let buf_ptr = source.as_ptr();
                line_offsets.extend(source.lines().map(|line| {
                    raw::LineOffset(unsafe { line.as_ptr().offset_from(buf_ptr) as usize } as u32)
                }));
                line_offsets.push(raw::LineOffset(source.len() as u32));
                let line_offsets_end = line_offsets.len() as u32;

                files.push((
                    name,
                    raw::File {
                        name_offset,
                        source_offset,
                        line_offsets_start,
                        line_offsets_end,
                    },
                ));
            }
        });
        files.sort_by_key(|(name, _file)| *name);

        // iterate over the tokens and create our index
        let mut last = None;
        tracing::trace_span!("create index").in_scope(|| {
            for token in tokens {
                let (min_line, min_col) = token.get_dst();
                let sp = SourcePosition::new(min_line, min_col);
                let file = token.get_source();
                let line = token.get_src_line();
                let scope = lookup_scope(&sp);

                let file_idx = match file {
                    Some(file) => files
                        .binary_search_by_key(&file, |(file_name, _)| file_name)
                        .map(|idx| idx as u32)
                        .unwrap_or(NO_FILE_SENTINEL),
                    None => NO_FILE_SENTINEL,
                };

                let scope_idx = match scope {
                    ScopeLookupResult::NamedScope(name) => std::cmp::min(
                        Self::insert_string(&mut string_bytes, &mut strings, name),
                        GLOBAL_SCOPE_SENTINEL,
                    ),
                    ScopeLookupResult::AnonymousScope => ANONYMOUS_SCOPE_SENTINEL,
                    ScopeLookupResult::Unknown => GLOBAL_SCOPE_SENTINEL,
                };

                let sl = raw::OriginalSourceLocation {
                    file_idx,
                    line,
                    scope_idx,
                };

                if last == Some(sl) {
                    continue;
                }
                mappings.push((
                    raw::MinifiedSourcePosition {
                        line: sp.line,
                        column: sp.column,
                    },
                    sl,
                ));
                last = Some(sl);
            }
        });

        let files = files.into_iter().map(|(_name, file)| file).collect();

        Ok(Self {
            string_bytes,
            files,
            line_offsets,
            mappings,
        })
    }

    /// Insert a string into this converter.
    ///
    /// If the string was already present, it is not added again. A newly added string
    /// is prefixed by its length in LEB128 encoding. The returned `u32`
    /// is the offset into the `string_bytes` field where the string is saved.
    fn insert_string(
        string_bytes: &mut Vec<u8>,
        strings: &mut HashMap<String, u32>,
        s: &str,
    ) -> u32 {
        if s.is_empty() {
            return u32::MAX;
        }
        if let Some(&offset) = strings.get(s) {
            return offset;
        }
        let string_offset = string_bytes.len() as u32;
        let string_len = s.len() as u64;
        leb128::write::unsigned(string_bytes, string_len).unwrap();
        string_bytes.extend(s.bytes());

        strings.insert(s.to_owned(), string_offset);
        string_offset
    }

    /// Serialize the converted data.
    ///
    /// This writes the SmCache binary format into the given [`Write`].
    #[tracing::instrument(level = "trace", name = "SmCacheWriter::serialize", skip_all)]
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = WriteWrapper::new(writer);

        let header = raw::Header {
            magic: raw::SMCACHE_MAGIC,
            version: raw::SMCACHE_VERSION,
            num_mappings: self.mappings.len() as u32,
            num_files: self.files.len() as u32,
            num_line_offsets: self.line_offsets.len() as u32,
            string_bytes: self.string_bytes.len() as u32,
            _reserved: [0; 8],
        };

        writer.write(header.as_bytes())?;
        writer.align()?;

        for (min_sp, _) in &self.mappings {
            writer.write(min_sp.as_bytes())?;
        }
        writer.align()?;

        for (_, orig_sl) in self.mappings {
            writer.write(orig_sl.as_bytes())?;
        }
        writer.align()?;

        writer.write(self.files.as_bytes())?;
        writer.align()?;

        writer.write(self.line_offsets.as_bytes())?;
        writer.align()?;

        writer.write(&self.string_bytes)?;

        Ok(())
    }
}

/// An Error that can happen when building a [`SmCache`].
#[derive(Debug)]
pub struct SmCacheWriterError(SmCacheErrorInner);

impl From<SmCacheErrorInner> for SmCacheWriterError {
    fn from(inner: SmCacheErrorInner) -> Self {
        SmCacheWriterError(inner)
    }
}

#[derive(Debug)]
enum SmCacheErrorInner {
    SourceMap(sourcemap::Error),
    ScopeIndex(ScopeIndexError),
    SourceContext(SourceContextError),
}

impl std::error::Error for SmCacheWriterError {}

impl std::fmt::Display for SmCacheWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            SmCacheErrorInner::SourceMap(e) => e.fmt(f),
            SmCacheErrorInner::ScopeIndex(e) => e.fmt(f),
            SmCacheErrorInner::SourceContext(e) => e.fmt(f),
        }
    }
}

struct WriteWrapper<W> {
    writer: W,
    position: usize,
}

impl<W: Write> WriteWrapper<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            position: 0,
        }
    }

    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let len = data.len();
        self.writer.write_all(data)?;
        self.position += len;
        Ok(len)
    }

    fn align(&mut self) -> std::io::Result<usize> {
        let buf = &[0u8; 7];
        let len = raw::align_to_eight(self.position);
        self.write(&buf[0..len])
    }
}
