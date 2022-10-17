use std::io::Write;

use itertools::Itertools;
use js_source_scopes::{
    extract_scope_names, NameResolver, ScopeIndex, ScopeIndexError, SourceContext,
    SourceContextError,
};
use sourcemap::DecodedMap;
use watto::{Pod, StringTable, Writer};

use super::raw;
use super::{ScopeLookupResult, SourcePosition};

/// A structure that allows quick resolution of minified source position
/// to the original source position it maps to.
pub struct SourceMapCacheWriter {
    string_table: StringTable,
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
        let scopes = match extract_scope_names(source) {
            Ok(scopes) => scopes,
            Err(err) => {
                let err: &dyn std::error::Error = &err;
                tracing::error!(error = err, "failed parsing minified source");
                // even if the minified source failed parsing, we can still use the information
                // from the sourcemap itself.
                vec![]
            }
        };

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
            DecodedMap::Regular(sm) => sm
                .sources()
                .zip_longest(sm.source_contents().map(Option::unwrap_or_default)),
            DecodedMap::Hermes(smh) => smh
                .sources()
                .zip_longest(smh.source_contents().map(Option::unwrap_or_default)),
            DecodedMap::Index(_smi) => unreachable!(),
        };

        let mut string_table = StringTable::new();
        let mut mappings = Vec::new();

        let mut line_offsets = vec![];
        let mut files = vec![];
        tracing::trace_span!("extract original files").in_scope(|| {
            for orig_file in orig_files {
                let (name, source) = orig_file.or_default();
                let name_offset = string_table.insert(name) as u32;
                let source_offset = string_table.insert(source) as u32;
                let line_offsets_start = line_offsets.len() as u32;
                Self::append_line_offsets(source, &mut line_offsets);
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

        // iterate over the tokens and create our index
        let mut last = None;
        tracing::trace_span!("create index").in_scope(|| {
            for token in tokens {
                let (min_line, min_col) = token.get_dst();
                let sp = SourcePosition::new(min_line, min_col);
                let line = token.get_src_line();
                let column = token.get_src_col();
                let scope = lookup_scope(&sp);
                let mut file_idx = token.get_src_id();

                if file_idx >= files.len() as u32 {
                    file_idx = raw::NO_FILE_SENTINEL;
                }

                let scope_idx = match scope {
                    ScopeLookupResult::NamedScope(name) => {
                        std::cmp::min(string_table.insert(name) as u32, raw::GLOBAL_SCOPE_SENTINEL)
                    }
                    ScopeLookupResult::AnonymousScope => raw::ANONYMOUS_SCOPE_SENTINEL,
                    ScopeLookupResult::Unknown => raw::GLOBAL_SCOPE_SENTINEL,
                };

                let name = token.get_name();
                let name_idx = match name {
                    Some(name) => string_table.insert(name) as u32,
                    None => raw::NO_NAME_SENTINEL,
                };

                let sl = raw::OriginalSourceLocation {
                    file_idx,
                    line,
                    column,
                    name_idx,
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
            string_table,
            files,
            line_offsets,
            mappings,
        })
    }

    /// Serialize the converted data.
    ///
    /// This writes the SourceMapCache binary format into the given [`Write`].
    #[tracing::instrument(level = "trace", name = "SourceMapCacheWriter::serialize", skip_all)]
    pub fn serialize<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut writer = Writer::new(writer);
        let string_bytes = self.string_table.into_bytes();

        let header = raw::Header {
            magic: raw::SOURCEMAPCACHE_MAGIC,
            version: raw::SOURCEMAPCACHE_VERSION,
            num_mappings: self.mappings.len() as u32,
            num_files: self.files.len() as u32,
            num_line_offsets: self.line_offsets.len() as u32,
            string_bytes: string_bytes.len() as u32,
            _reserved: [0; 8],
        };

        writer.write_all(header.as_bytes())?;
        writer.align_to(8)?;

        for (min_sp, _) in &self.mappings {
            writer.write_all(min_sp.as_bytes())?;
        }
        writer.align_to(8)?;

        for (_, orig_sl) in self.mappings {
            writer.write_all(orig_sl.as_bytes())?;
        }
        writer.align_to(8)?;

        writer.write_all(self.files.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(self.line_offsets.as_bytes())?;
        writer.align_to(8)?;

        writer.write_all(&string_bytes)?;

        Ok(())
    }

    /// Compute line offsets for a source file and append them to the given  vector.
    ///
    /// There is always one line offset at the start of the file (even if the file is empty)
    /// and then another one after every newline (even if the file ends on a newline).
    pub(crate) fn append_line_offsets(source: &str, out: &mut Vec<raw::LineOffset>) {
        // The empty file has only one line offset for the start.
        if source.is_empty() {
            out.push(raw::LineOffset(0));
            return;
        }

        let buf_ptr = source.as_ptr();
        out.extend(source.lines().map(move |line| {
            raw::LineOffset(unsafe { line.as_ptr().offset_from(buf_ptr) as usize } as u32)
        }));

        // If the file ends with a line break, add another line offset for the empty last line
        // (the lines iterator skips it).
        if source.ends_with('\n') {
            out.push(raw::LineOffset(source.len() as u32));
        }
    }
}

/// An Error that can happen when building a [`SourceMapCache`](super::SourceMapCache).
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

#[cfg(test)]
mod tests {

    use super::*;
    use crate::raw::LineOffset;

    #[test]
    fn line_offsets_empty_file() {
        let source = "";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        assert_eq!(line_offsets, [LineOffset(0)]);
    }

    #[test]
    fn line_offsets_almost_empty_file() {
        let source = "\n";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        assert_eq!(line_offsets, [LineOffset(0), LineOffset(1)]);
    }

    #[test]
    fn line_offsets_several_lines() {
        let source = "a\n\nb\nc";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        assert_eq!(
            line_offsets,
            [LineOffset(0), LineOffset(2), LineOffset(3), LineOffset(5),]
        );
    }

    #[test]
    fn line_offsets_several_lines_trailing_newline() {
        let source = "a\n\nb\nc\n";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

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
