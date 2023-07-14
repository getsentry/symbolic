use std::cmp::Ordering;
use std::collections::BTreeMap;

/// A pattern for matching source paths.
///
/// A pattern either matches a string exactly (`Exact`)
/// or it matches any string starting with a certain prefix (`Prefix`).
///
/// Patterns are ordered as follows:
/// 1. Exact patterns come before prefixes
/// 2. Exact patterns are ordered lexicographically
/// 3. Prefix patterns are ordered inversely by length, i.e.,
///    longer before shorter, and lexicographically among equally long strings.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    Exact(String),
    Prefix(String),
}

impl Pattern {
    fn parse(input: &str) -> Self {
        if let Some(prefix) = input.strip_suffix('*') {
            Pattern::Prefix(prefix.to_lowercase())
        } else {
            Pattern::Exact(input.to_lowercase())
        }
    }
}

impl Ord for Pattern {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Pattern::Exact(s), Pattern::Exact(t)) => s.cmp(t),
            (Pattern::Exact(_), Pattern::Prefix(_)) => Ordering::Less,
            (Pattern::Prefix(_), Pattern::Exact(_)) => Ordering::Greater,
            (Pattern::Prefix(s), Pattern::Prefix(t)) => match s.len().cmp(&t.len()) {
                Ordering::Greater => Ordering::Less,
                Ordering::Equal => s.cmp(t),
                Ordering::Less => Ordering::Greater,
            },
        }
    }
}

impl PartialOrd for Pattern {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A structure mapping source file paths to remote locations.
///
/// # Example
/// ```
/// use symbolic_common::SourceLinkMappings;
/// let mappings = vec![
///     ("C:\\src\\*", "http://MyDefaultDomain.com/src/*"),
///     ("C:\\src\\fOO\\*", "http://MyFooDomain.com/src/*"),
///     ("C:\\src\\foo\\specific.txt", "http://MySpecificFoodDomain.com/src/specific.txt"),
///     ("C:\\src\\bar\\*", "http://MyBarDomain.com/src/*"),
/// ];
/// let mappings = SourceLinkMappings::new(mappings.into_iter());
/// let resolved = mappings.resolve("c:\\src\\bAr\\foo\\FiLe.txt").unwrap();
/// assert_eq!(resolved, "http://MyBarDomain.com/src/foo/FiLe.txt");
/// ````
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SourceLinkMappings {
    mappings: BTreeMap<Pattern, String>,
}

impl<'a> Extend<(&'a str, &'a str)> for SourceLinkMappings {
    fn extend<T: IntoIterator<Item = (&'a str, &'a str)>>(&mut self, iter: T) {
        self.mappings.extend(
            iter.into_iter()
                .map(|(k, v)| (Pattern::parse(k), v.to_string())),
        )
    }
}

impl SourceLinkMappings {
    /// Creates a `SourceLinkMappings` struct from an iterator of pattern/target pairs.
    pub fn new<'a, I: IntoIterator<Item = (&'a str, &'a str)>>(iter: I) -> Self {
        let mut res = Self::default();
        res.extend(iter);
        res
    }
    /// Returns true if this structure contains no mappings.
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    /// Resolve the path to a URL.
    pub fn resolve(&self, path: &str) -> Option<String> {
        // Note: this is currently quite simple, just pick the first match. If we needed to improve
        // performance in the future because we encounter PDBs with too many items, we can do a
        // prefix binary search, for example.
        let path_lower = path.to_lowercase();
        for (pattern, target) in &self.mappings {
            match &pattern {
                Pattern::Exact(value) => {
                    if value == &path_lower {
                        return Some(target.clone());
                    }
                }
                Pattern::Prefix(value) => {
                    if path_lower.starts_with(value) {
                        let replacement = path
                            .get(value.len()..)
                            .unwrap_or_default()
                            .replace('\\', "/");
                        return Some(target.replace('*', &replacement));
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapping() {
        let mappings = vec![
            ("C:\\src\\*", "http://MyDefaultDomain.com/src/*"),
            ("C:\\src\\fOO\\*", "http://MyFooDomain.com/src/*"),
            (
                "C:\\src\\foo\\specific.txt",
                "http://MySpecificFoodDomain.com/src/specific.txt",
            ),
            ("C:\\src\\bar\\*", "http://MyBarDomain.com/src/*"),
            ("C:\\src\\file.txt", "https://example.com/file.txt"),
            ("/home/user/src/*", "https://linux.com/*"),
        ];

        let mappings = SourceLinkMappings::new(mappings.into_iter());

        assert_eq!(mappings.mappings.len(), 6);

        // In this example:
        //   All files under directory bar will map to a relative URL beginning with http://MyBarDomain.com/src/.
        //   All files under directory foo will map to a relative URL beginning with http://MyFooDomain.com/src/ EXCEPT foo/specific.txt which will map to http://MySpecificFoodDomain.com/src/specific.txt.
        //   All other files anywhere under the src directory will map to a relative url beginning with http://MyDefaultDomain.com/src/.
        assert!(mappings.resolve("c:\\other\\path").is_none());
        assert!(mappings.resolve("/home/path").is_none());
        assert_eq!(
            mappings.resolve("c:\\src\\bAr\\foo\\FiLe.txt").unwrap(),
            "http://MyBarDomain.com/src/foo/FiLe.txt"
        );
        assert_eq!(
            mappings.resolve("c:\\src\\foo\\FiLe.txt").unwrap(),
            "http://MyFooDomain.com/src/FiLe.txt"
        );
        assert_eq!(
            mappings.resolve("c:\\src\\foo\\SpEcIfIc.txt").unwrap(),
            "http://MySpecificFoodDomain.com/src/specific.txt"
        );
        assert_eq!(
            mappings.resolve("c:\\src\\other\\path").unwrap(),
            "http://MyDefaultDomain.com/src/other/path"
        );
        assert_eq!(
            mappings.resolve("c:\\src\\other\\path").unwrap(),
            "http://MyDefaultDomain.com/src/other/path"
        );
        assert_eq!(
            mappings.resolve("/home/user/src/Path/TO/file.txt").unwrap(),
            "https://linux.com/Path/TO/file.txt"
        );
    }
}
