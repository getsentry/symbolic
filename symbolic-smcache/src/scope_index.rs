use std::ops::Range;

use indexmap::IndexSet;

/// An indexed structure of Scopes that allows quick lookup.
///
/// Primarily, it converts a list of possibly nested named scopes into an
/// optimized structure that allows quick lookup.
/// Construction of the index will validate that the scopes are well nested and
/// parents fully contain their children. A list of scopes that are not well
/// nested will result in an `Err` on construction.
///
/// # Examples
///
/// ```
/// use symbolic_smcache::{ScopeIndex, ScopeLookupResult};
///
/// let scopes = vec![
///     (5..25, Some(String::from("parent"))),
///     (10..15, Some(String::from("child"))),
///     (20..25, Some(String::from("child2"))),
///     (30..50, None),
/// ];
///
/// let idx = ScopeIndex::new(scopes).unwrap();
/// assert_eq!(idx.lookup(3), ScopeLookupResult::Unknown);
/// assert_eq!(idx.lookup(12), ScopeLookupResult::NamedScope("child"));
/// assert_eq!(idx.lookup(40), ScopeLookupResult::AnonymousScope);
/// ```
#[derive(Debug)]
pub struct ScopeIndex {
    names: IndexSet<String>,
    /// Offset -> Index into `names` (or `u32::MAX` for `None`)
    ranges: Vec<(u32, u32)>,
}

impl ScopeIndex {
    /// Creates a new Scope index from the given list of Scopes.
    #[tracing::instrument(level = "trace", name = "ScopeIndex::new", skip_all)]
    pub fn new(mut scopes: Vec<(Range<u32>, Option<String>)>) -> Result<Self, ScopeIndexError> {
        let mut names = IndexSet::new();
        let mut ranges = vec![];

        scopes.sort_by_key(|s| s.0.start);

        let needs_zero = scopes.first().map(|s| s.0.start != 0).unwrap_or(false);
        if needs_zero {
            ranges.push((0, GLOBAL_SCOPE_SENTINEL));
        }

        let mut stack: Vec<(Range<u32>, u32)> = vec![];

        for (range, name) in scopes {
            unwind_scope_stack(&mut ranges, &mut stack, range.clone())?;

            let name_idx = match name {
                Some(name) => names
                    .insert_full(name)
                    .0
                    .try_into()
                    .map_err(|_| ScopeIndexError(()))?,
                None => ANONYMOUS_SCOPE_SENTINEL,
            };

            ranges.push((range.start, name_idx));

            if let Some(last) = stack.last() {
                if last.0.end == range.end {
                    stack.pop();
                }
            }
            stack.push((range, name_idx));
        }

        // push end markers for the remaining stack
        while let Some(last) = stack.pop() {
            // push a new range of the parent
            let name_idx = stack
                .last()
                .map(|prev| prev.1)
                .unwrap_or(GLOBAL_SCOPE_SENTINEL);
            ranges.push((last.0.end, name_idx));
        }

        Ok(Self { names, ranges })
    }

    /// Looks up the scope corresponding to the given `offset`.
    pub fn lookup(&self, offset: u32) -> ScopeLookupResult {
        let range_idx = match self.ranges.binary_search_by_key(&offset, |r| r.0) {
            Ok(idx) => idx,
            Err(0) => 0, // this is pretty much unreachable since the first offset is 0
            Err(idx) => idx - 1,
        };

        let name_idx = match self.ranges.get(range_idx) {
            Some(r) => r.1,
            None => return ScopeLookupResult::Unknown,
        };

        self.resolve_name(name_idx)
    }

    fn resolve_name(&self, name_idx: u32) -> ScopeLookupResult {
        if name_idx == GLOBAL_SCOPE_SENTINEL {
            ScopeLookupResult::Unknown
        } else if name_idx == ANONYMOUS_SCOPE_SENTINEL {
            ScopeLookupResult::AnonymousScope
        } else {
            match self.names.get_index(name_idx as usize) {
                Some(name) => ScopeLookupResult::NamedScope(name.as_str()),
                None => ScopeLookupResult::Unknown,
            }
        }
    }

    /// Iterates over all the offsets and scope names in order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, ScopeLookupResult)> {
        self.ranges.iter().map(|r| (r.0, self.resolve_name(r.1)))
    }
}

/// The Result of a Scope lookup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScopeLookupResult<'data> {
    /// A named function scope.
    NamedScope(&'data str),
    /// An anonymous function scope for which no name was inferred.
    AnonymousScope,
    /// The lookup did not result in any scope match.
    ///
    /// This most likely means that the offset belongs to the "global" scope.
    Unknown,
}

/// Given a `stack` of ranges, this pushes all entries on the stack
/// to `ranges` that end before `offset`, and ensures well-nestedness.
fn unwind_scope_stack(
    ranges: &mut Vec<(u32, u32)>,
    stack: &mut Vec<(Range<u32>, u32)>,
    range: Range<u32>,
) -> Result<(), ScopeIndexError> {
    while let Some(last) = stack.pop() {
        // push a new range of the parent
        if last.0.end <= range.start {
            let name_idx = stack
                .last()
                .map(|prev| prev.1)
                .unwrap_or(GLOBAL_SCOPE_SENTINEL);
            ranges.push((last.0.end, name_idx));
        } else if last.0.end < range.end {
            // we have an overlap and improper nesting
            return Err(ScopeIndexError(()));
        } else {
            // re-push to the stack, as it is still our same parent
            stack.push(last);
            return Ok(());
        }
    }
    Ok(())
}

pub(crate) const GLOBAL_SCOPE_SENTINEL: u32 = u32::MAX;
pub(crate) const ANONYMOUS_SCOPE_SENTINEL: u32 = u32::MAX - 1;

/// An Error that can happen when building a [`ScopeIndex`].
#[derive(Debug)]
pub struct ScopeIndexError(());

impl std::error::Error for ScopeIndexError {}

impl std::fmt::Display for ScopeIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source could not be converted to source context")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_nesting() {
        let scopes = vec![(0..10, None), (5..15, None)];
        assert!(ScopeIndex::new(scopes).is_err());
    }

    #[test]
    fn scope_index() {
        let scopes = vec![
            (5..25, Some(String::from("parent"))),
            (10..15, Some(String::from("child"))),
            (20..25, Some(String::from("child2"))),
            (30..50, None),
        ];

        let idx = ScopeIndex::new(scopes).unwrap();

        assert_eq!(idx.lookup(3), ScopeLookupResult::Unknown);
        assert_eq!(idx.lookup(7), ScopeLookupResult::NamedScope("parent"));
        assert_eq!(idx.lookup(12), ScopeLookupResult::NamedScope("child"));
        assert_eq!(idx.lookup(17), ScopeLookupResult::NamedScope("parent"));
        assert_eq!(idx.lookup(22), ScopeLookupResult::NamedScope("child2"));
        assert_eq!(idx.lookup(25), ScopeLookupResult::Unknown);
        assert_eq!(idx.lookup(30), ScopeLookupResult::AnonymousScope);
        assert_eq!(idx.lookup(50), ScopeLookupResult::Unknown);
    }
}
