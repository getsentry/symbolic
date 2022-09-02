use std::collections::HashMap;
use std::io::Write;

use sourcemap::DecodedMap;
use zerocopy::AsBytes;

use crate::scope_index::{ScopeIndex, ScopeIndexError, ScopeLookupResult};
use crate::source::{SourceContext, SourceContextError};
use crate::{extract_scope_names, NameResolver, SourcePosition};

use super::raw;
use raw::{ANONYMOUS_SCOPE_SENTINEL, GLOBAL_SCOPE_SENTINEL, NO_FILE_SENTINEL};

/// A structure that allows quick resolution of minified source position
/// to the original source position it maps to.
pub struct SourceMapCacheWriter {
    string_bytes: Vec<u8>,
    files: Vec<raw::File>,
    line_offsets: Vec<raw::LineOffset>,
    mappings: Vec<(raw::MinifiedSourcePosition, raw::OriginalSourceLocation)>,
}

impl SourceMapCacheWriter {
    /// Constructs a new Cache from a minified source file and its corresponding SourceMap.
    #[tracing::instrument(level = "trace", name = "SourceMapCacheWriter::new", skip_all)]
    pub fn new(source: &str, sourcemap: &str) -> Result<Self, SourceMapCacheWriterError> {
        let sm = tracing::trace_span!("decode sourcemap").in_scope(
            || -> Result<DecodedMap, SourceMapCacheWriterError> {
                let sm = sourcemap::decode_slice(sourcemap.as_bytes())
                    .map_err(SourceMapCacheErrorInner::SourceMap)?;
                // flatten the `SourceMapIndex`, as we want to iterate tokens
                Ok(match sm {
                    DecodedMap::Regular(sm) => DecodedMap::Regular(sm),
                    DecodedMap::Index(smi) => DecodedMap::Regular(
                        smi.flatten().map_err(SourceMapCacheErrorInner::SourceMap)?,
                    ),
                    DecodedMap::Hermes(smh) => DecodedMap::Hermes(smh),
                })
            },
        )?;

        let tokens = match &sm {
            DecodedMap::Regular(sm) => sm.tokens(),
            DecodedMap::Hermes(smh) => smh.tokens(),
            DecodedMap::Index(_smi) => unreachable!(),
        };

        // Hermes/Metro SourceMaps have scope information embedded in them which we can use.
        // In that case, we can skip parsing the minified source, which in most cases is empty / non-existent
        // as Hermes ships bytecode that we are not able to parse anyway.
        // Skipping this whole code would be nice, but that gets us into borrow-checker hell, so
        // just clearing the minified source skips the whole code there anyways.
        let source = if matches!(&sm, DecodedMap::Hermes(_)) {
            ""
        } else {
            source
        };

        // parse scopes out of the minified source
        let scopes = extract_scope_names(source);

        // resolve scopes to original names
        let ctx = SourceContext::new(source).map_err(SourceMapCacheErrorInner::SourceContext)?;
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
        let scope_index = ScopeIndex::new(scopes).map_err(SourceMapCacheErrorInner::ScopeIndex)?;
        let scope_index: Vec<_> = tracing::trace_span!("convert scope index").in_scope(|| {
            scope_index
                .iter()
                .filter_map(|(offset, result)| {
                    let pos = ctx.offset_to_position(offset);
                    pos.map(|pos| (pos, result))
                })
                .collect()
        });
        let lookup_scope = |sp: &SourcePosition| {
            if let DecodedMap::Hermes(smh) = &sm {
                let token = smh.lookup_token(sp.line, sp.column);
                return match token.and_then(|token| smh.get_scope_for_token(token)) {
                    Some(name) => ScopeLookupResult::NamedScope(name),
                    None => ScopeLookupResult::Unknown,
                };
            }

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
                line_offsets.extend(Self::line_offsets(source));
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
                let column = token.get_src_col();
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
                    column,
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
    /// This writes the SourceMapCache binary format into the given [`Write`].
    #[tracing::instrument(level = "trace", name = "SourceMapCacheWriter::serialize", skip_all)]
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = WriteWrapper::new(writer);

        let header = raw::Header {
            magic: raw::SOURCEMAPCACHE_MAGIC,
            version: raw::SOURCEMAPCACHE_VERSION,
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

    /// Compute line offsets for a source file.
    ///
    /// There is always one line offset at the start of the file (even if the file is empty)
    /// and then another one after every newline (even if the file ends on a newline).
    pub(crate) fn line_offsets(source: &str) -> impl Iterator<Item = raw::LineOffset> + '_ {
        let buf_ptr = source.as_ptr();
        source
            .is_empty()
            .then(|| raw::LineOffset(0))
            .into_iter()
            .chain(source.lines().map(move |line| {
                raw::LineOffset(unsafe { line.as_ptr().offset_from(buf_ptr) as usize } as u32)
            }))
            .chain(
                // If the file ends with a line break, add another line offset for the empty last line
                // (the lines iterator skips it).
                source
                    .ends_with('\n')
                    .then(|| raw::LineOffset(source.len() as u32)),
            )
    }
}

/// An Error that can happen when building a [`super::SourceMapCache`].
#[derive(Debug)]
pub struct SourceMapCacheWriterError(SourceMapCacheErrorInner);

impl From<SourceMapCacheErrorInner> for SourceMapCacheWriterError {
    fn from(inner: SourceMapCacheErrorInner) -> Self {
        SourceMapCacheWriterError(inner)
    }
}

#[derive(Debug)]
pub(crate) enum SourceMapCacheErrorInner {
    SourceMap(sourcemap::Error),
    ScopeIndex(ScopeIndexError),
    SourceContext(SourceContextError),
}

impl std::error::Error for SourceMapCacheWriterError {}

impl std::fmt::Display for SourceMapCacheWriterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            SourceMapCacheErrorInner::SourceMap(e) => e.fmt(f),
            SourceMapCacheErrorInner::ScopeIndex(e) => e.fmt(f),
            SourceMapCacheErrorInner::SourceContext(e) => e.fmt(f),
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
#[cfg(test)]
mod tests {

    use super::*;
    use crate::sourcemapcache::raw::LineOffset;

    #[test]
    fn line_offsets_empty_file() {
        let source = "";

        let line_offsets = SourceMapCacheWriter::line_offsets(source).collect::<Vec<_>>();

        assert_eq!(line_offsets, [LineOffset(0)]);
    }

    #[test]
    fn line_offsets_almost_empty_file() {
        let source = "\n";

        let line_offsets = SourceMapCacheWriter::line_offsets(source).collect::<Vec<_>>();

        assert_eq!(line_offsets, [LineOffset(0), LineOffset(1)]);
    }

    #[test]
    fn line_offsets_several_lines() {
        let source = "a\n\nb\nc";

        let line_offsets = SourceMapCacheWriter::line_offsets(source).collect::<Vec<_>>();

        assert_eq!(
            line_offsets,
            [LineOffset(0), LineOffset(2), LineOffset(3), LineOffset(5),]
        );
    }

    #[test]
    fn line_offsets_several_lines_trailing_newline() {
        let source = "a\n\nb\nc\n";

        let line_offsets = SourceMapCacheWriter::line_offsets(source).collect::<Vec<_>>();

        assert_eq!(
            line_offsets,
            [
                LineOffset(0),
                LineOffset(2),
                LineOffset(3),
                LineOffset(5),
                LineOffset(7),
            ]
        );
    }
}
