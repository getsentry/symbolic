use std::collections::BTreeSet;
use std::fs;

use failure::{Fail, ResultExt};

use symbolic_common::{clean_path, join_path};
use symbolic_debuginfo::{DebugSession, ObjectLike};

use crate::bundle::{ArtifactBundleWriter, ArtifactFileInfo, ArtifactType};
use crate::error::{ArtifactBundleError, ArtifactBundleErrorKind};

/// Writes sources of `Object` files to an artifact bundle.
pub struct DebugSourceWriter {
    bundle: ArtifactBundleWriter,
    files_handled: BTreeSet<String>,
}

impl DebugSourceWriter {
    /// Creates a new source writer around an artifact bundle writer.
    pub fn new(bundle: ArtifactBundleWriter) -> Self {
        DebugSourceWriter {
            bundle,
            files_handled: BTreeSet::new(),
        }
    }

    /// Writes all source files referenced by functions in this object file to the bundle.
    pub fn write_object<O>(&mut self, object: &O) -> Result<(), ArtifactBundleError>
    where
        O: ObjectLike,
        O::Error: Fail,
    {
        // TODO: Add attributes to the bundle.

        let mut session = object
            .debug_session()
            .context(ArtifactBundleErrorKind::BadDebugFile)?;

        self.bundle
            .set_attribute("debug_id", object.debug_id().to_string());
        for func in session.functions() {
            let func = func.context(ArtifactBundleErrorKind::BadDebugFile)?;
            for line in &func.lines {
                let filename = clean_path(&join_path(
                    &String::from_utf8_lossy(&func.compilation_dir),
                    &line.file.path_str(),
                ));
                if self.files_handled.contains(&filename) {
                    continue;
                }
                let source = if filename.starts_with('<') && filename.ends_with('>') {
                    None
                } else {
                    fs::read_to_string(&filename).ok()
                };
                if let Some(source) = source {
                    self.bundle
                        .add_file(
                            &filename,
                            source.as_bytes(),
                            ArtifactFileInfo {
                                ty: Some(ArtifactType::Script),
                                path: filename.clone(),
                                headers: Default::default(),
                            },
                        )
                        .context(ArtifactBundleErrorKind::WriteFailed)?;
                }
                self.files_handled.insert(filename);
            }
        }

        Ok(())
    }

    /// Finishes writing the object file and returns the bundle writer.
    pub fn finish(self) -> Result<ArtifactBundleWriter, ArtifactBundleError> {
        Ok(self.bundle)
    }
}
