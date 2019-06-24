use std::collections::BTreeSet;

use failure::{Fail, ResultExt};

use symbolic_debuginfo::{DebugSession, ObjectLike};

use crate::bundle::ArtifactBundleWriter;
use crate::error::{ArtifactBundleError, ArtifactBundleErrorKind};

/// Writes sources of `Object` files to an artifact bundle.
pub struct DebugSourceWriter {
    bundle: ArtifactBundleWriter,
    ignored_files: BTreeSet<String>,
}

impl DebugSourceWriter {
    /// Creates a new source writer around an artifact bundle writer.
    pub fn new(bundle: ArtifactBundleWriter) -> Self {
        DebugSourceWriter {
            bundle,
            ignored_files: BTreeSet::new(),
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

        for function in session.functions() {
            println!("TODO: {:#?}", function);
        }

        Ok(())
    }

    /// Finishes writing the object file and returns the bundle writer.
    pub fn finish(self) -> Result<ArtifactBundleWriter, ArtifactBundleError> {
        Ok(self.bundle)
    }
}
