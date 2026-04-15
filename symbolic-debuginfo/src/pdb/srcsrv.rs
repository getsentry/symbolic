//! Support for source server information in PDB files.

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;

use crate::pdb::{PdbError, PdbErrorKind};

/// Source server information for a particular path.
#[derive(Debug, Clone)]
pub(crate) struct SourceServerInfo {
    /// The file's path on the source server.
    pub(crate) path: String,
    /// The file's revision.
    pub(crate) revision: Option<String>,
}

/// VCS schemas for which symbolic can
/// process source server information.
#[derive(Debug, Clone)]
enum SourceServerVcs {
    /// Perforce.
    ///
    /// For perforce, we require the following layout:
    /// * `var3` contains the depot path
    /// * `var4` contains the changelist
    Perforce,
    /// Any other VCS.
    Unknown(String),
}

impl SourceServerVcs {
    /// The VCS's name.
    pub(crate) fn name(&self) -> &str {
        match self {
            SourceServerVcs::Perforce => "Perforce",
            SourceServerVcs::Unknown(s) => s,
        }
    }
}

impl FromStr for SourceServerVcs {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("perforce") {
            Ok(Self::Perforce)
        } else {
            Ok(Self::Unknown(s.to_owned()))
        }
    }
}

/// A parsed source server stream that can be used to look up remapping
/// information for a path.
pub(crate) struct SourceServerMappings<'s> {
    /// The VCS schema in the stream.
    vcs: SourceServerVcs,
    /// A cache for already computed [`SourceServerInfos`](SourceServerInfo).
    cache: RefCell<HashMap<String, Option<SourceServerInfo>>>,
    /// The underlying `SrcSrvStream`.
    stream: srcsrv::SrcSrvStream<'s>,
}

impl<'s> SourceServerMappings<'s> {
    /// Attempt to parse a slice of bytes into [`SourceServerMappings`].
    ///
    /// This can fail if the `srcsrv_stream` is malformed or lacks a
    /// `"verctrl"` field.
    pub(crate) fn parse(srcsrv_stream: &'s [u8]) -> Result<Self, PdbError> {
        let stream = srcsrv::SrcSrvStream::parse(srcsrv_stream)
            .map_err(|e| PdbError::new(PdbErrorKind::BadObject, e))?;
        let vcs = stream
            .version_control_description()
            .ok_or(PdbErrorKind::MissingSourceServerVcs)?
            .parse()
            .unwrap_or_else(|e| match e {});

        Ok(Self {
            vcs,
            cache: Default::default(),
            stream,
        })
    }

    /// Freshly compute remapping information for a path from the underlying
    /// [`SrcSrvStream`].
    ///
    /// The computation method depends on the `vcs`.
    fn compute_info(
        vcs: &SourceServerVcs,
        stream: &srcsrv::SrcSrvStream,
        path: &str,
    ) -> Option<SourceServerInfo> {
        let Ok(Some((_method, var_map))) = stream.source_and_raw_var_values_for_path(path, "")
        else {
            return None;
        };

        match vcs {
            // Extracts depot path (var3) and changelist (var4), then returns
            // a tuple of (path, optional revision).
            SourceServerVcs::Perforce => {
                let depot_path = var_map.get("var3")?;
                let changelist = var_map.get("var4");

                // Strip leading // from depot path for code mapping compatibility
                let depot = depot_path.trim_start_matches("//");
                let revision = changelist.filter(|cl| !cl.is_empty());

                Some(SourceServerInfo {
                    path: depot.to_owned(),
                    revision: revision.map(|s| s.to_owned()),
                })
            }
            SourceServerVcs::Unknown(_) => None,
        }
    }

    /// Returns remapping information for the given path.
    ///
    /// This caches the information internally.
    pub(crate) fn get_info(&self, path: &str) -> Option<SourceServerInfo> {
        self.cache
            .borrow_mut()
            .entry(path.to_owned())
            .or_insert_with(|| Self::compute_info(&self.vcs, &self.stream, path))
            .clone()
    }

    /// Returns the name of the VCS in this stream.
    pub(crate) fn vcs_name(&self) -> &str {
        self.vcs.name()
    }
}
