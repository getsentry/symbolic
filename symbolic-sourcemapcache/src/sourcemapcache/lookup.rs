use zerocopy::LayoutVerified;

use crate::{ScopeLookupResult, SourcePosition};

use super::raw;

/// A resolved Source Location with file, line, column and scope information.
#[derive(Debug, PartialEq)]
pub struct SourceLocation<'data> {
    /// The source file this location belongs to.
    file: Option<File<'data>>,
    /// The source line.
    line: u32,
    /// The source column.
    column: u32,
    /// The scope containing this source location.
    scope: ScopeLookupResult<'data>,
}

impl<'data> SourceLocation<'data> {
    /// The source file this location belongs to.
    pub fn file(&self) -> Option<File<'data>> {
        self.file
    }

    /// The number of the source line.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The number of the source column.
    pub fn column(&self) -> u32 {
        self.column
    }

    /// The contents of the source line.
    pub fn line_contents(&self) -> Option<&'data str> {
        self.file().and_then(|file| file.line(self.line as usize))
    }

    /// The scope containing this source location.
    pub fn scope(&self) -> ScopeLookupResult<'data> {
        self.scope
    }

    /// The name of the source file this location belongs to.
    pub fn file_name(&self) -> Option<&'data str> {
        self.file.map(|file| file.name)
    }

    /// The source of the file this location belongs to.
    pub fn file_source(&self) -> Option<&'data str> {
        self.file.and_then(|file| file.source)
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone)]
pub struct SourceMapCache<'data> {
    header: &'data raw::Header,
    min_source_positions: &'data [raw::MinifiedSourcePosition],
    orig_source_locations: &'data [raw::OriginalSourceLocation],
    files: &'data [raw::File],
    line_offsets: &'data [raw::LineOffset],
    string_bytes: &'data [u8],
}

impl<'data> std::fmt::Debug for SourceMapCache<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceMapCache")
            .field("version", &self.header.version)
            .field("mappings", &self.header.num_mappings)
            .field("files", &self.header.num_files)
            .field("line_offsets", &self.header.num_line_offsets)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

impl<'data> SourceMapCache<'data> {
    #[tracing::instrument(level = "trace", name = "SourceMapCache::parse", skip_all)]
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (header, buf): (LayoutVerified<_, raw::Header>, _) =
            LayoutVerified::new_from_prefix(buf).ok_or(Error::Header)?;
        let header = header.into_ref();
        let buf = align_buf(buf);

        if header.magic == raw::SOURCEMAPCACHE_MAGIC_FLIPPED {
            return Err(Error::WrongEndianness);
        }
        if header.magic != raw::SOURCEMAPCACHE_MAGIC {
            return Err(Error::WrongFormat);
        }
        if header.version != raw::SOURCEMAPCACHE_VERSION {
            return Err(Error::WrongVersion);
        }

        let num_mappings = header.num_mappings as usize;
        let (min_source_positions, buf) = LayoutVerified::new_slice_from_prefix(buf, num_mappings)
            .ok_or(Error::SourcePositions)?;
        let min_source_positions = min_source_positions.into_slice();
        let buf = align_buf(buf);

        let (orig_source_locations, buf) = LayoutVerified::new_slice_from_prefix(buf, num_mappings)
            .ok_or(Error::SourceLocations)?;
        let orig_source_locations = orig_source_locations.into_slice();
        let buf = align_buf(buf);

        let (files, buf) = LayoutVerified::new_slice_from_prefix(buf, header.num_files as usize)
            .ok_or(Error::SourceLocations)?;
        let files = files.into_slice();
        let buf = align_buf(buf);

        let (line_offsets, buf) =
            LayoutVerified::new_slice_from_prefix(buf, header.num_line_offsets as usize)
                .ok_or(Error::SourceLocations)?;
        let line_offsets = line_offsets.into_slice();
        let buf = align_buf(buf);

        let string_bytes = header.string_bytes as usize;
        let string_bytes = buf.get(..string_bytes).ok_or(Error::StringBytes)?;

        Ok(Self {
            header,
            min_source_positions,
            orig_source_locations,
            files,
            line_offsets,
            string_bytes,
        })
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        let reader = &mut self.string_bytes.get(offset as usize..)?;
        let len = leb128::read::unsigned(reader).ok()? as usize;

        let bytes = reader.get(..len)?;

        std::str::from_utf8(bytes).ok()
    }

    fn resolve_file(&self, raw_file: &raw::File) -> Option<File<'data>> {
        let name = self.get_string(raw_file.name_offset)?;
        let source = self.get_string(raw_file.source_offset);
        let line_offsets = self
            .line_offsets
            .get(raw_file.line_offsets_start as usize..raw_file.line_offsets_end as usize)?;
        Some(File {
            name,
            source,
            line_offsets,
        })
    }

    /// Looks up a [`SourcePosition`] in the minified source and resolves it
    /// to the original [`SourceLocation`].
    #[tracing::instrument(level = "trace", name = "SourceMapCache::lookup", skip_all)]
    pub fn lookup(&self, sp: SourcePosition) -> Option<SourceLocation> {
        let idx = match self.min_source_positions.binary_search(&sp.into()) {
            Ok(idx) => idx,
            Err(0) => 0,
            Err(idx) => idx - 1,
        };

        let sl = self.orig_source_locations.get(idx)?;

        let line = sl.line;
        let column = sl.column;

        let file = self
            .files
            .get(sl.file_idx as usize)
            .and_then(|raw_file| self.resolve_file(raw_file));

        let scope = match sl.scope_idx {
            raw::GLOBAL_SCOPE_SENTINEL => ScopeLookupResult::Unknown,
            raw::ANONYMOUS_SCOPE_SENTINEL => ScopeLookupResult::AnonymousScope,
            idx => self
                .get_string(idx)
                .map_or(ScopeLookupResult::Unknown, ScopeLookupResult::NamedScope),
        };

        Some(SourceLocation {
            file,
            line,
            column,
            scope,
        })
    }

    /// Returns an iterator over all files in the cache.
    pub fn files(&self) -> Files<'data> {
        Files::new(self)
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The file was generated by a system with different endianness.
    #[error("endianness mismatch")]
    WrongEndianness,
    /// The file magic does not match.
    #[error("wrong format magic")]
    WrongFormat,
    /// The format version in the header is wrong/unknown.
    #[error("unknown SymCache version")]
    WrongVersion,
    #[error("invalid header")]
    Header,
    #[error("invalid source positions")]
    SourcePositions,
    #[error("invalid source locations")]
    SourceLocations,
    #[error("invalid string bytes")]
    StringBytes,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct File<'data> {
    name: &'data str,
    source: Option<&'data str>,
    line_offsets: &'data [raw::LineOffset],
}

impl<'data> File<'data> {
    /// Returns the name of this file.
    pub fn name(&self) -> &'data str {
        self.name
    }

    /// Returns the source of this file.
    pub fn source(&self) -> Option<&'data str> {
        self.source
    }

    /// Returns the requested source line if possible.
    pub fn line(&self, line_no: usize) -> Option<&'data str> {
        let source = self.source?;
        let from = self.line_offsets.get(line_no).copied()?.0 as usize;
        let next_line_no = line_no.checked_add(1);
        let to = next_line_no
            .and_then(|next_line_no| self.line_offsets.get(next_line_no))
            .map_or(source.len(), |lo| lo.0 as usize);
        source.get(from..to)
    }
}

/// Iterator returned by [`SourceMapCache::files`].
pub struct Files<'data> {
    cache: SourceMapCache<'data>,
    raw_files: std::slice::Iter<'data, raw::File>,
}

impl<'data> Files<'data> {
    fn new(cache: &SourceMapCache<'data>) -> Self {
        Self {
            cache: cache.clone(),
            raw_files: cache.files.iter(),
        }
    }
}

impl<'data> Iterator for Files<'data> {
    type Item = File<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.raw_files
            .next()
            .and_then(|raw_file| self.cache.resolve_file(raw_file))
    }
}

fn align_buf(buf: &[u8]) -> &[u8] {
    let offset = buf.as_ptr().align_offset(8);
    buf.get(offset..).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceMapCacheWriter;

    #[test]
    fn lines_empty_file() {
        let source = "";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        let file = File {
            name: "empty",
            source: Some(source),
            line_offsets: &line_offsets,
        };

        assert_eq!(file.line(0), Some(""));
        assert_eq!(file.line(1), None);
    }

    #[test]
    fn lines_almost_empty_file() {
        let source = "\n";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        let file = File {
            name: "almost_empty",
            source: Some(source),
            line_offsets: &line_offsets,
        };

        assert_eq!(file.line(0), Some("\n"));
        assert_eq!(file.line(1), Some(""));
        assert_eq!(file.line(2), None);
    }

    #[test]
    fn lines_several_lines() {
        let source = "a\n\nb\nc";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        let file = File {
            name: "several_lines",
            source: Some(source),
            line_offsets: &line_offsets,
        };

        assert_eq!(file.line(0), Some("a\n"));
        assert_eq!(file.line(1), Some("\n"));
        assert_eq!(file.line(2), Some("b\n"));
        assert_eq!(file.line(3), Some("c"));
    }

    #[test]
    fn lines_several_lines_trailing_newline() {
        let source = "a\n\nb\nc\n";
        let mut line_offsets = Vec::new();
        SourceMapCacheWriter::append_line_offsets(source, &mut line_offsets);

        let file = File {
            name: "several_lines_trailing_newline",
            source: Some(source),
            line_offsets: &line_offsets,
        };

        assert_eq!(file.line(0), Some("a\n"));
        assert_eq!(file.line(1), Some("\n"));
        assert_eq!(file.line(2), Some("b\n"));
        assert_eq!(file.line(3), Some("c\n"));
        assert_eq!(file.line(4), Some(""));
    }
}
