use zerocopy::LayoutVerified;

use crate::{ScopeLookupResult, SourcePosition};

use super::raw;

/// A resolved Source Location  with file, line and scope information.
#[derive(Debug, PartialEq)]
pub struct SourceLocation<'data> {
    /// The source file this location belongs to.
    file: Option<File<'data>>,
    /// The source line.
    line: u32,
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
pub struct SmCache<'data> {
    header: &'data raw::Header,
    min_source_positions: &'data [raw::MinifiedSourcePosition],
    orig_source_locations: &'data [raw::OriginalSourceLocation],
    files: &'data [raw::File],
    line_offsets: &'data [raw::LineOffset],
    string_bytes: &'data [u8],
}

impl<'data> std::fmt::Debug for SmCache<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmCache")
            .field("version", &self.header.version)
            .field("mappings", &self.header.num_mappings)
            .field("files", &self.header.num_files)
            .field("line_offsets", &self.header.num_line_offsets)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

impl<'data> SmCache<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (header, buf): (LayoutVerified<_, raw::Header>, _) =
            LayoutVerified::new_from_prefix(buf).ok_or(Error::Header)?;
        let header = header.into_ref();
        let buf = align_buf(buf);

        if header.magic == raw::SMCACHE_MAGIC_FLIPPED {
            return Err(Error::WrongEndianness);
        }
        if header.magic != raw::SMCACHE_MAGIC {
            return Err(Error::WrongFormat);
        }
        if header.version != raw::SMCACHE_VERSION {
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
    pub fn lookup(&self, sp: SourcePosition) -> Option<SourceLocation> {
        let idx = match self.min_source_positions.binary_search(&sp.into()) {
            Ok(idx) => idx,
            Err(0) => 0,
            Err(idx) => idx - 1,
        };

        let sl = self.orig_source_locations.get(idx)?;

        let line = sl.line;

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

        Some(SourceLocation { file, line, scope })
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

#[derive(Debug, PartialEq, Clone, Copy)]
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
        let from = self.line_offsets.get(line_no).copied()?.0 as usize;
        let to = self.line_offsets.get(line_no.checked_add(1)?).copied()?.0 as usize;
        self.source.and_then(|source| source.get(from..to))
    }
}

/// Iterator returned by [`SmCache::files`].
pub struct Files<'data> {
    cache: SmCache<'data>,
    raw_files: std::slice::Iter<'data, raw::File>,
}

impl<'data> Files<'data> {
    fn new(cache: &SmCache<'data>) -> Self {
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
