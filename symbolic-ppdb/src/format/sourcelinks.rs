use std::cmp::Ordering;

use crate::{FormatError, FormatErrorKind};

/// See [Source Link PPDB docs](https://github.com/dotnet/designs/blob/main/accepted/2020/diagnostics/source-link.md#source-link-json-schema).
#[derive(Clone)]
pub(crate) struct SourceLinkMappings {
    rules: Vec<Rule>,
    sorted: bool,
}

#[derive(Clone)]
struct Rule {
    pattern: Pattern,
    url: String,
}

#[derive(Clone)]
enum Pattern {
    Exact(String),
    Prefix(String),
}

impl SourceLinkMappings {
    pub fn empty() -> Self {
        SourceLinkMappings {
            rules: Vec::new(),
            sorted: false,
        }
    }

    pub fn add_mappings(&mut self, json: &str) -> Result<(), FormatError> {
        use serde_json::*;
        let json: Value = serde_json::from_str(json)
            .map_err(|e| FormatError::new(FormatErrorKind::InvalidSourceLinkJson, e))?;

        let docs = json
            .get("documents")
            .and_then(|v| v.as_object())
            .ok_or_else(Self::err)?;

        self.rules.reserve(docs.len());
        for doc in docs.iter() {
            /*
            Each document is defined by a file path and a URL. Original source file paths are compared
            case-insensitively to documents and the resulting URL is used to download source. The document
            may contain an asterisk to represent a wildcard in order to match anything in the asterisk's
            location. The rules for the asterisk are as follows:
                1. The only acceptable wildcard is one and only one '*', which if present will be replaced by a relative path.
                2. If the file path does not contain a *, the URL cannot contain a * and if the file path contains a * the URL must contain a *.
                3. If the file path contains a *, it must be the final character.
                4. If the URL contains a *, it may be anywhere in the URL.
            */
            let key = doc.0.to_lowercase();
            let url = doc.1.as_str().ok_or_else(Self::err)?.into();
            let pattern = if let Some(prefix) = key.strip_suffix('*') {
                Pattern::Prefix(prefix.into())
            } else {
                Pattern::Exact(key)
            };
            self.rules.push(Rule { pattern, url });
            self.sorted = false;
        }
        Ok(())
    }

    fn err() -> FormatError {
        FormatError {
            kind: FormatErrorKind::InvalidSourceLinkJson,
            source: None,
        }
    }

    /// Sort internal rules. This must be called before [Self::resolve].
    pub fn sort(&mut self) {
        // Put Exact matches first, then sort by the Prefix length, longest to shortest.
        self.rules.sort_unstable_by(|a, b| match &a.pattern {
            Pattern::Exact(_) => Ordering::Less,
            Pattern::Prefix(a) => match &b.pattern {
                Pattern::Exact(_) => Ordering::Greater,
                Pattern::Prefix(b) => b.len().cmp(&a.len()),
            },
        });
        self.sorted = true;
    }

    /// Resolve the path to a URL.
    pub fn resolve(&self, path: &str) -> Option<String> {
        // Must be sorted first so we can return on the first match, which is guaranteed to be the most specific.
        if !self.sorted {
            return None;
        }

        // Note: this is currently quite simple, just pick the first match. If we needed to improve
        // performance in the future because we encounter PDBs with too many items, we can do a
        // prefix binary search, for example.
        let path_lower = path.to_lowercase();
        for rule in &self.rules {
            match &rule.pattern {
                Pattern::Exact(value) => {
                    if value == &path_lower {
                        return Some(rule.url.clone());
                    }
                }
                Pattern::Prefix(value) => {
                    if path_lower.starts_with(value) {
                        let replacement = path
                            .get(value.len()..)
                            .map(|v| v.replace('\\', "/"))
                            .unwrap_or_default();
                        return Some(rule.url.replace('*', &replacement));
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
    fn test_invalid_json() {
        let mut mappings = SourceLinkMappings::empty();
        assert!(mappings.add_mappings("").is_err());
        assert!(mappings.add_mappings("foo").is_err());
        assert!(mappings.add_mappings("{\"docs\": {\"k\": \"v\"}}").is_err());
        assert!(mappings
            .add_mappings("{\"documents\": [\"k\", \"v\"]}")
            .is_err());
        assert_eq!(mappings.rules.len(), 0);
    }

    #[test]
    fn test_mapping() {
        let mut mappings = SourceLinkMappings::empty();
        mappings
            .add_mappings(
                r#"
                {
                    "documents": {
                        "C:\\src\\*":                   "http://MyDefaultDomain.com/src/*",
                        "C:\\src\\fOO\\*":              "http://MyFooDomain.com/src/*",
                        "C:\\src\\foo\\specific.txt":   "http://MySpecificFoodDomain.com/src/specific.txt",
                        "C:\\src\\bar\\*":              "http://MyBarDomain.com/src/*"
                    }
                }
                "#
            ).unwrap();

        assert_eq!(mappings.rules.len(), 4);
        assert!(!mappings.sorted);
        mappings.sort();
        assert!(mappings.sorted);

        mappings
            .add_mappings(
                r#"
                {
                    "documents": {
                        "C:\\src\\file.txt": "https://example.com/file.txt"
                    }
                }
                "#,
            )
            .unwrap();

        assert_eq!(mappings.rules.len(), 5);
        assert!(!mappings.sorted);
        mappings.sort();
        assert!(mappings.sorted);

        // In this example:
        //   All files under directory bar will map to a relative URL beginning with http://MyBarDomain.com/src/.
        //   All files under directory foo will map to a relative URL beginning with http://MyFooDomain.com/src/ EXCEPT foo/specific.txt which will map to http://MySpecificFoodDomain.com/src/specific.txt.
        //   All other files anywhere under the src directory will map to a relative url beginning with http://MyDefaultDomain.com/src/.
        assert!(mappings.resolve("c:\\other\\path").is_none());
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
    }
}
