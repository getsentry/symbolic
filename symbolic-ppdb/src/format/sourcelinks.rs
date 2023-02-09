use std::cmp::Ordering;

use crate::{FormatError, FormatErrorKind};

/// See https://github.com/dotnet/designs/blob/main/accepted/2020/diagnostics/source-link.md#source-link-json-schema
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
            let key = doc.0;
            let url = doc.1.as_str().ok_or_else(Self::err)?.into();
            let pattern = if key.ends_with('*') {
                Pattern::Prefix(key[..(key.len() - 1)].into())
            } else {
                Pattern::Exact(key.into())
            };
            self.rules.push(Rule { pattern, url })
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

        for rule in &self.rules {
            if match &rule.pattern {
                Pattern::Exact(value) => value == path,
                Pattern::Prefix(value) => path.starts_with(value),
            } {
                return Some(rule.url.clone());
            }
        }
        None
    }
}
