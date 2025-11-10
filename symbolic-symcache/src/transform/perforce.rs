//! Resolves local build paths to Perforce depot paths using SRCSRV metadata
//! embedded in PDB files.
//!
//! This transformer is used in game development where builds happen on different
//! machines. The build system embeds Perforce metadata into PDB files, which maps
//! local build paths (e.g., `C:\build\game\src\main.cpp`) to depot paths
//! (e.g., `//depot/game/src/main.cpp`) along with revision numbers.

use std::borrow::Cow;
use std::collections::HashMap;

use symbolic_common::clean_path;

use super::{File, Function, SourceLocation, Transformer};

/// Information extracted from a Perforce SRCSRV entry
#[derive(Debug, Clone)]
struct PerforceEntry {
    /// Depot path (e.g., `//depot/project/src/file.cpp`)
    depot_path: String,
    /// Revision number
    revision: String,
}

/// Maps local build paths to Perforce depot paths using SRCSRV data.
///
/// This transformer extracts path mappings from the SRCSRV stream embedded in
/// PDB files and transforms local file paths to depot paths during symcache writing.
pub struct PerforcePathMapper {
    /// Map from normalized local path to Perforce depot info
    path_map: HashMap<String, PerforceEntry>,
}

impl PerforcePathMapper {
    /// Create a new Perforce path mapper from raw SRCSRV data.
    ///
    /// This directly parses the SRCSRV format to extract Perforce mappings.
    /// Returns `None` if the SRCSRV data is not for Perforce (VERCTRL != Perforce)
    /// or if no valid mappings are found.
    ///
    /// ## SRCSRV Format
    ///
    /// The SRCSRV stream has two sections:
    /// - `SRCSRV: variables` - Defines variables like P4_CUSTOM_EDGE (server address)
    /// - `SRCSRV: source files` - Contains path mappings
    ///
    /// Source file format: `local_path*server_var*depot_path*revision`
    /// Example: `C:\build\src\main.cpp*P4_CUSTOM_EDGE*depot/src/main.cpp*42`
    ///
    /// We extract fields 3 (depot_path) and 4 (revision) as literal values.
    /// Field 2 (server_var) is a variable reference (e.g., P4_CUSTOM_EDGE) that
    /// debuggers resolve to a server address, but we don't need it for path transformation.
    pub fn from_srcsrv_data(data: &str) -> Option<Self> {
        // First, verify this is Perforce SRCSRV data by checking VERCTRL
        let mut is_perforce = false;

        for line in data.lines() {
            let line = line.trim();
            if line.starts_with("VERCTRL=") {
                // Check if VERCTRL is set to Perforce (case-insensitive)
                let verctrl_value = line.strip_prefix("VERCTRL=")?.trim();
                is_perforce = verctrl_value.eq_ignore_ascii_case("Perforce");
                break;
            }
        }

        // If not Perforce SRCSRV data, return None
        if !is_perforce {
            return None;
        }

        let mut path_map = HashMap::new();
        let mut in_files_section = false;

        for line in data.lines() {
            let line = line.trim();

            // Look for the source files section
            if line.starts_with("SRCSRV: source files") {
                in_files_section = true;
                continue;
            } else if line.starts_with("SRCSRV: end") {
                break;
            } else if line.starts_with("SRCSRV:") {
                in_files_section = false;
                continue;
            }

            // Parse source file entries
            // Format: local_path*server*depot_path*revision
            // Example: C:\build\src\main.cpp*P4_CUSTOM_EDGE*depot/src/main.cpp*42
            // Note: Field 2 (server) is just an identifier/marker - we skip it
            // Fields 3 and 4 are already literal values (not variable references)
            if in_files_section && !line.is_empty() {
                let parts: Vec<&str> = line.split('*').collect();
                if parts.len() >= 4 {
                    let local_path = parts[0];
                    let depot_path = parts[2];  // Skip parts[1] (server identifier)
                    let revision = parts[3];

                    // Normalize the local path for case-insensitive matching
                    let normalized = normalize_path(local_path);

                    // Ensure depot path starts with //
                    let depot_path = if depot_path.starts_with("//") {
                        depot_path.to_string()
                    } else {
                        format!("//{}", depot_path)
                    };

                    path_map.insert(
                        normalized,
                        PerforceEntry {
                            depot_path,
                            revision: revision.to_string(),
                        },
                    );
                }
            }
        }

        if path_map.is_empty() {
            None
        } else {
            Some(PerforcePathMapper { path_map })
        }
    }

    /// Try to remap a file path to a Perforce depot path.
    ///
    /// Returns `(depot_path, revision)` if a mapping is found.
    fn remap_path(&self, file: &File<'_>) -> Option<(String, String)> {
        // Reconstruct full path from comp_dir + directory + name
        let comp_dir = file.comp_dir.as_deref().unwrap_or_default();
        let directory = file.directory.as_deref().unwrap_or_default();
        let path_name = &file.name;

        // Try different path combinations
        let full_path = join_path(comp_dir, &join_path(directory, path_name));
        let normalized = normalize_path(&clean_path(&full_path));

        // Look up in path map
        if let Some(entry) = self.path_map.get(&normalized) {
            return Some((entry.depot_path.clone(), entry.revision.clone()));
        }

        // Try without comp_dir
        let without_comp = join_path(directory, path_name);
        let normalized = normalize_path(&clean_path(&without_comp));
        if let Some(entry) = self.path_map.get(&normalized) {
            return Some((entry.depot_path.clone(), entry.revision.clone()));
        }

        // Try just the filename
        let normalized = normalize_path(path_name);
        if let Some(entry) = self.path_map.get(&normalized) {
            return Some((entry.depot_path.clone(), entry.revision.clone()));
        }

        None
    }
}

impl Transformer for PerforcePathMapper {
    fn transform_function<'f>(&'f mut self, f: Function<'f>) -> Function<'f> {
        // Functions don't need transformation for Perforce
        f
    }

    fn transform_source_location<'f>(
        &'f mut self,
        mut sl: SourceLocation<'f>,
    ) -> SourceLocation<'f> {
        if let Some((depot_path, revision)) = self.remap_path(&sl.file) {
            // Replace file path with depot path
            sl.file.name = Cow::Owned(depot_path);

            // Clear directory components as the depot path is absolute
            sl.file.comp_dir = None;
            sl.file.directory = None;

            // Store revision in comp_dir for now (we could extend SourceLocation later)
            // Format: revision:<number>
            sl.file.comp_dir = Some(Cow::Owned(format!("revision:{}", revision)));
        }

        sl
    }
}

/// Normalize a file path for case-insensitive matching on Windows
fn normalize_path(path: &str) -> String {
    path.to_lowercase().replace('\\', "/")
}

/// Join two path components
fn join_path(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else if b.is_empty() {
        a.to_string()
    } else {
        format!("{}/{}", a.trim_end_matches('/'), b.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(r"C:\Build\Game\src\main.cpp"), "c:/build/game/src/main.cpp");
        assert_eq!(normalize_path("//depot/game/main.cpp"), "//depot/game/main.cpp");
    }

    #[test]
    fn test_join_path() {
        assert_eq!(join_path("a", "b"), "a/b");
        assert_eq!(join_path("a/", "/b"), "a/b");
        assert_eq!(join_path("", "b"), "b");
        assert_eq!(join_path("a", ""), "a");
    }

    #[test]
    fn test_from_srcsrv_data() {
        let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: variables ------------------------------------------
P4_EXTRACT_CMD=p4.exe -p %fnvar%(%var2%) print -o %srcsrvtrg% -q "//%var3%#%var4%"
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*P4_CUSTOM_EDGE*depot/game/src/main.cpp*42
C:\build\game\src\util.cpp*P4_CUSTOM_EDGE*depot/game/src/util.cpp*43
SRCSRV: end ------------------------------------------------
"#;

        let mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data).unwrap();
        assert_eq!(mapper.path_map.len(), 2);

        let entry = mapper.path_map.get("c:/build/game/src/main.cpp").unwrap();
        assert_eq!(entry.depot_path, "//depot/game/src/main.cpp");
        assert_eq!(entry.revision, "42");
    }

    #[test]
    fn test_transformation() {
        use std::borrow::Cow;

        let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*P4_EDGE*depot/game/src/main.cpp*42
SRCSRV: end ------------------------------------------------
"#;

        let mut mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data).unwrap();

        // Create a source location that matches the SRCSRV entry
        let source_loc = SourceLocation {
            file: File {
                name: Cow::Borrowed("main.cpp"),
                directory: Some(Cow::Borrowed("src")),
                comp_dir: Some(Cow::Borrowed("C:/build/game")),
            },
            line: 10,
        };

        // Transform it
        let transformed = mapper.transform_source_location(source_loc);

        // Verify transformation
        assert_eq!(transformed.file.name, "//depot/game/src/main.cpp");
        assert_eq!(transformed.file.directory, None);
        assert_eq!(transformed.file.comp_dir, Some(Cow::Borrowed("revision:42")));
        assert_eq!(transformed.line, 10);
    }

    #[test]
    fn test_no_match() {
        use std::borrow::Cow;

        let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*P4_EDGE*depot/game/src/main.cpp*42
SRCSRV: end ------------------------------------------------
"#;

        let mut mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data).unwrap();

        // Create a source location that doesn't match
        let source_loc = SourceLocation {
            file: File {
                name: Cow::Borrowed("unknown.cpp"),
                directory: Some(Cow::Borrowed("other")),
                comp_dir: Some(Cow::Borrowed("D:/different/path")),
            },
            line: 5,
        };

        // Transform it
        let transformed = mapper.transform_source_location(source_loc);

        // Verify no transformation occurred
        assert_eq!(transformed.file.name, "unknown.cpp");
        assert_eq!(transformed.file.directory, Some(Cow::Borrowed("other")));
        assert_eq!(transformed.file.comp_dir, Some(Cow::Borrowed("D:/different/path")));
        assert_eq!(transformed.line, 5);
    }

    #[test]
    fn test_empty_srcsrv() {
        let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: variables ------------------------------------------
SRCSRV: source files ---------------------------------------
SRCSRV: end ------------------------------------------------
"#;

        let mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data);
        assert!(mapper.is_none(), "Should return None for empty source files");
    }

    #[test]
    fn test_depot_path_normalization() {
        let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: source files ---------------------------------------
C:\build\test.cpp*P4_EDGE*depot/project/test.cpp*100
SRCSRV: end ------------------------------------------------
"#;

        let mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data).unwrap();

        // Verify depot path was normalized to start with //
        let entry = mapper.path_map.get("c:/build/test.cpp").unwrap();
        assert_eq!(entry.depot_path, "//depot/project/test.cpp");
        assert_eq!(entry.revision, "100");
    }
}
