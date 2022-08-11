/// A Source Context allowing fast line/column <-> byte offset remapping.
///
/// The primary use-case is to allow efficient conversion between
/// [`SourcePosition`]s (line/column) to byte offsets. The [`SourcePosition`]s
/// are 0-based. All offsets are treated as `u32`, and creating a
/// [`SourceContext`] for a source that exceeds the range of a `u32` will result
/// in an `Err`.
///
/// # Examples
///
/// ```
/// use symbolic_smcache::{SourceContext, SourcePosition};
///
/// let src = r#"const arrowFnExpr = (a) => a;
/// function namedFnDecl() {}"#;
///
/// let ctx = SourceContext::new(src).unwrap();
///
/// let offset = ctx.position_to_offset(SourcePosition::new(0, 6)).unwrap() as usize;
/// assert_eq!(&src[offset..offset+11], "arrowFnExpr");
/// let offset = ctx.position_to_offset(SourcePosition::new(1, 9)).unwrap() as usize;
/// assert_eq!(&src[offset..offset+11], "namedFnDecl");
/// ```
pub struct SourceContext<T> {
    src: T,
    index: Vec<Mapping>,
}

/// When creating the [`SourceContext`], create a mapping every [`CHUNKS`] char.
///
/// For example for a 80kiB byte file, we would have 640 of these mappings,
/// weighing about 7k in memory.
const CHUNKS: usize = 128;

/// A mapping in the [`SourceContext`] index.
#[derive(Clone, Copy)]
struct Mapping {
    /// The current byte offset.
    offset: u32,
    /// Current 0-indexed line.
    line: u32,
    /// Current 0-indexed UTF-16 column.
    column: u32,
}

impl<T: AsRef<str>> SourceContext<T> {
    /// Unwrap this Source Context into the inner source buffer.
    pub fn into_inner(self) -> T {
        self.src
    }

    /// Construct a new Source Context from the given `src` buffer.
    #[tracing::instrument(level = "trace", name = "SourceContext::new", skip_all)]
    pub fn new(src: T) -> Result<Self, SourceContextError> {
        let buf = src.as_ref();
        // we can do the bounds check once in the beginning, that guarantees that
        // all the other offsets are within `u32` bounds.
        let _len: u32 = buf.len().try_into().map_err(|_| SourceContextError(()))?;

        let mut index = vec![];

        let mut offset = 0;
        let mut line = 0;
        let mut column = 0;
        for (i, c) in buf.chars().enumerate() {
            if i % CHUNKS == 0 {
                index.push(Mapping {
                    offset: offset as u32,
                    line,
                    column: column as u32,
                });
            }
            offset += c.len_utf8();
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += c.len_utf16();
            }
        }

        Ok(Self { src, index })
    }

    /// Converts a byte offset into the source to the corresponding line/column.
    ///
    /// The column is given in UTF-16 code points.
    pub fn offset_to_position(&self, offset: u32) -> Option<SourcePosition> {
        let mapping = match self
            .index
            .binary_search_by_key(&offset, |mapping| mapping.offset)
        {
            Ok(idx) => self.index[idx],
            Err(0) => Mapping {
                offset: 0,
                line: 0,
                column: 0,
            },
            Err(idx) => self.index[idx - 1],
        };

        let mut byte_offset = mapping.offset as usize;
        let mut line = mapping.line;
        let mut column = mapping.column as usize;

        for c in self.src.as_ref().get(byte_offset..)?.chars() {
            if byte_offset >= offset as usize {
                return Some(SourcePosition::new(line, column as u32));
            }

            byte_offset += c.len_utf8();
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += c.len_utf16();
            }
        }

        None
    }

    /// Converts the given line/column to the corresponding byte offset inside the source.
    pub fn position_to_offset(&self, position: SourcePosition) -> Option<u32> {
        let SourcePosition { line, column } = position;
        let mapping = match self
            .index
            .binary_search_by_key(&(line, column), |mapping| (mapping.line, mapping.column))
        {
            Ok(idx) => self.index[idx],
            Err(0) => Mapping {
                offset: 0,
                line: 0,
                column: 0,
            },
            Err(idx) => self.index[idx - 1],
        };

        let mut byte_offset = mapping.offset as usize;
        let mut mapping_line = mapping.line;
        let mut mapping_column = mapping.column as usize;

        for c in self.src.as_ref().get(byte_offset..)?.chars() {
            if mapping_line == line && mapping_column >= column as usize {
                return Some(byte_offset as u32);
            }

            byte_offset += c.len_utf8();
            if c == '\n' {
                mapping_line += 1;
                mapping_column = 0;
                // the column we were looking for is out of bounds
                if mapping_line > line {
                    return None;
                }
            } else {
                mapping_column += c.len_utf16();
            }
        }

        None
    }
}

/// A line/column source position.
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct SourcePosition {
    /// Line in the source file, 0-based.
    pub line: u32,
    /// Column in the source file, 0-based.
    ///
    /// The column is given in UTF-16 code points.
    pub column: u32,
}

impl SourcePosition {
    /// Create a new SourcePosition with the given line/column.
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// An Error that can happen when building a [`SourceContext`].
#[derive(Debug)]
pub struct SourceContextError(());

impl std::error::Error for SourceContextError {}

impl std::fmt::Display for SourceContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source could not be converted to source context")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_context() {
        let ctx = SourceContext::new("").unwrap();
        assert_eq!(ctx.offset_to_position(0), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(0, 0)), None);

        let src = "\n \r\naö¿¡\nőá…–🤮🚀¿ 한글 테스트\nz̴̢̈͜ä̴̺̟́ͅl̸̛̦͎̺͂̃̚͝g̷̦̲͊͋̄̌͝o̸͇̞̪͙̞͌̇̀̓̏͜\r\noh hai";
        let ctx = SourceContext::new(src).unwrap();

        // out of bounds
        assert_eq!(ctx.offset_to_position(150), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(0, 1)), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(1, 3)), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(6, 1)), None);

        // correct positions
        assert_eq!(ctx.offset_to_position(1), Some(SourcePosition::new(1, 0)));
        assert_eq!(ctx.offset_to_position(3), Some(SourcePosition::new(1, 2)));

        let offset = ctx.position_to_offset(SourcePosition::new(2, 0)).unwrap();
        assert_eq!(offset, 4);
        assert_eq!(&src[offset as usize..(offset as usize + 1)], "a");

        // full roundtrips
        for (offset, _c) in src.char_indices() {
            if let Some(sp) = ctx.offset_to_position(offset as u32) {
                let roundtrip = ctx.position_to_offset(sp).unwrap();
                assert_eq!(roundtrip, offset as u32);
            }
        }
    }
}
