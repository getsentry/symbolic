//! Types & Definitions needed to keep compatibility with existing API

use super::*;

impl<'data> SymCache<'data> {
    /// An iterator over the functions in this SymCache.
    pub fn functions(&self) -> Functions<'data> {
        Functions {
            cache: self.clone(),
            function_idx: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Functions<'data> {
    cache: SymCache<'data>,
    function_idx: u32,
}

impl<'data> Iterator for Functions<'data> {
    type Item = Function<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        self.cache.get_function(self.function_idx).map(|file| {
            self.function_idx += 1;
            file
        })
    }
}
