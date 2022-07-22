/// A Source Context allowing fast access to lines and line/column <-> byte offset remapping.
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
    line_offsets: Vec<u32>,
}

impl<T: AsRef<str>> SourceContext<T> {
    /// Unwrap this Source Context into the inner source buffer.
    pub fn into_inner(self) -> T {
        self.src
    }

    /// Construct a new Source Context from the given `src` buffer.
    pub fn new(src: T) -> Result<Self, SourceContextError> {
        let buf = src.as_ref();
        // we can do the bounds check once in the beginning, that guarantees that
        // all the other offsets are within `u32` bounds.
        let len = buf.len().try_into().map_err(|_| SourceContextError(()))?;

        let buf_ptr = buf.as_ptr();
        let mut line_offsets: Vec<u32> = buf
            .lines()
            .map(|line| unsafe { line.as_ptr().offset_from(buf_ptr) as usize } as u32)
            .collect();
        line_offsets.push(len);
        Ok(Self { src, line_offsets })
    }

    /// Get the `nth` line of the source, 0-based.
    pub fn get_line(&self, nth: u32) -> Option<&str> {
        let from = self.line_offsets.get(nth as usize).copied()? as usize;
        let to = self.line_offsets.get(nth as usize + 1).copied()? as usize;
        self.src.as_ref().get(from..to)
    }

    /// Converts a byte offset into the source to the corresponding line/column.
    ///
    /// The column is given in UTF-16 code points.
    pub fn offset_to_position(&self, offset: u32) -> Option<SourcePosition> {
        let line_no = match self.line_offsets.binary_search(&offset) {
            Ok(line) => line,
            Err(0) => 0, // this is pretty much unreachable since the first offset is 0
            Err(line) => line - 1,
        };

        let mut byte_offset = self.line_offsets.get(line_no).copied()? as usize;
        let to = self.line_offsets.get(line_no + 1).copied()? as usize;

        let line = self.src.as_ref().get(byte_offset..to)?;

        let line_no = line_no.try_into().ok()?;
        let mut utf16_offset = 0;
        for c in line.chars() {
            if byte_offset >= offset as usize {
                return Some(SourcePosition::new(line_no, utf16_offset.try_into().ok()?));
            }

            utf16_offset += c.len_utf16();
            byte_offset += c.len_utf8();
        }

        None
    }

    /// Converts the given line/column to the corresponding byte offset inside the source.
    pub fn position_to_offset(&self, position: SourcePosition) -> Option<u32> {
        let SourcePosition { line, column } = position;

        let from = self.line_offsets.get(line as usize).copied()? as usize;
        let to = self.line_offsets.get(line as usize + 1).copied()? as usize;

        let line = self.src.as_ref().get(from..to)?;

        let mut byte_offset = from;
        let mut utf16_offset = 0;
        let column = column as usize;
        for c in line.chars() {
            if utf16_offset >= column {
                return byte_offset.try_into().ok();
            }
            utf16_offset += c.len_utf16();
            byte_offset += c.len_utf8();
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
        assert_eq!(ctx.get_line(0), None);
        assert_eq!(ctx.offset_to_position(0), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(0, 0)), None);

        let src = "\n \r\na";
        let ctx = SourceContext::new(src).unwrap();

        // lines
        assert_eq!(ctx.get_line(0), Some("\n"));
        assert_eq!(ctx.get_line(1), Some(" \r\n"));
        assert_eq!(ctx.get_line(2), Some("a"));

        // out of bounds
        assert_eq!(ctx.offset_to_position(5), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(0, 1)), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(1, 3)), None);
        assert_eq!(ctx.position_to_offset(SourcePosition::new(2, 1)), None);

        // correct positions
        assert_eq!(ctx.offset_to_position(1), Some(SourcePosition::new(1, 0)));
        assert_eq!(ctx.offset_to_position(3), Some(SourcePosition::new(1, 2)));

        let offset = ctx.position_to_offset(SourcePosition::new(2, 0)).unwrap();
        assert_eq!(offset, 4);
        assert_eq!(&src[offset as usize..], "a");

        // full roundtrips
        for offset in 0..=src.len() as u32 {
            if let Some(sp) = ctx.offset_to_position(offset) {
                let roundtrip = ctx.position_to_offset(sp).unwrap();
                assert_eq!(roundtrip, offset);
            }
        }
    }
}
