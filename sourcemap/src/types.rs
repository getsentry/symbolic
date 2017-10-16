use sourcemap;

use symbolic_common::Result;

pub use sourcemap::SourceView;


/// Represents a sourcemap.
pub struct SourceMap {
    sm: sourcemap::SourceMap,
}

/// A matched token.
pub struct TokenMatch<'a> {
    pub src_line: u32,
    pub src_col: u32,
    pub dst_line: u32,
    pub dst_col: u32,
    pub name: Option<&'a str>,
    pub src: Option<&'a str>,
    pub function_name: Option<String>,
}

impl SourceMap {
    /// Constructs a sourcemap from a slice.
    ///
    /// If the sourcemap is an index it is being flattened.  If flattening
    /// is not possible then an error is raised.
    pub fn from_slice(buffer: &[u8]) -> Result<SourceMap> {
        Ok(SourceMap {
            sm: match sourcemap::decode_slice(buffer)? {
                sourcemap::DecodedMap::Regular(sm) => sm,
                sourcemap::DecodedMap::Index(smi) => smi.flatten()?,
            }
        })
    }

    /// Looks up a token and returns it.
    pub fn lookup_token<'a>(&'a self, line: u32, col: u32) -> Option<TokenMatch<'a>> {
        self.sm.lookup_token(line, col).map(|tok| {
            self.make_token_match(tok)
        })
    }

    /// Looks up a token and the original function name.
    ///
    /// This is similar to `lookup_token` but if a minified function name and
    /// the sourceview to the minified source is available this function will
    /// also resolve the original function name.  This is used to fully
    /// resolve tracebacks.
    pub fn lookup_token_with_function_name<'a, 'b>(&'a self, line: u32, col: u32,
                                                   minified_name: &str,
                                                   source: &SourceView<'b>)
        -> Option<TokenMatch<'a>>
    {
        self.sm.lookup_token(line, col).map(|token| {
            let mut rv = self.make_token_match(token);
            rv.function_name = source
                .get_original_function_name(token, minified_name)
                .map(|x| x.to_string());
            rv
        })
    }

    fn make_token_match<'a>(&'a self, tok: sourcemap::Token<'a>) -> TokenMatch<'a> {
        TokenMatch {
            src_line: tok.get_src_line(),
            src_col: tok.get_src_col(),
            dst_line: tok.get_dst_line(),
            dst_col: tok.get_dst_col(),
            name: tok.get_name(),
            src: tok.get_source(),
            function_name: None,
        }
    }
}
