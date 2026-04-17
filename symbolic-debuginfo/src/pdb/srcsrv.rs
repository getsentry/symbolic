//! Support for source server information in PDB files.

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::str::FromStr;

use crate::pdb::{PdbError, PdbErrorKind};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SourceServerError {
    /// The srcsrv stream doesn't contain a VCS name.
    #[error("missing VCS name in srcsrv stream")]
    MissingSourceServerVcs,
}

/// Source server information for a particular path.
#[derive(Debug, Clone)]
pub struct SourceServerInfo {
    /// The file's path on the source server.
    pub path: String,
    /// The file's revision.
    pub revision: Option<String>,
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
    pub fn name(&self) -> &str {
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

/// A parsed source server stream that can be used to look up
/// a file's revision and path on the source server.
pub struct SourceServerMappings<'s> {
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
    pub fn parse(srcsrv_stream: &'s [u8]) -> Result<Self, PdbError> {
        let stream = srcsrv::SrcSrvStream::parse(srcsrv_stream)
            .map_err(|e| PdbError::new(PdbErrorKind::BadObject, e))?;
        let vcs = stream
            .version_control_description()
            .ok_or(PdbError::new(
                PdbErrorKind::BadObject,
                SourceServerError::MissingSourceServerVcs,
            ))?
            .parse()
            .unwrap_or_else(|e| match e {});

        Ok(Self {
            vcs,
            cache: Default::default(),
            stream,
        })
    }

    /// Freshly compute source server information for a path from the underlying
    /// [`SrcSrvStream`](srcsrv::SrcSrvStream).
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
            // Extracts depot path (var3) and changelist (var4).
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

    /// Returns source server information for the given path.
    ///
    /// This caches the information internally.
    pub fn get_info(&self, path: &str) -> Option<SourceServerInfo> {
        self.cache
            .borrow_mut()
            .entry(path.to_owned())
            .or_insert_with(|| Self::compute_info(&self.vcs, &self.stream, path))
            .clone()
    }

    /// Returns the name of the VCS in this stream.
    pub fn vcs_name(&self) -> &str {
        self.vcs.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perforce_mapping() {
        let stream = br#"SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: variables ------------------------------------------
SRCSRVTRG=%targ%\%var2%\%fnbksl%(%var3%)
SRCSRVCMD=
SRCSRV: source files ---------------------------------------
c:\projects\breakpad-tools\deps\breakpad\src\client\windows\crash_generation\crash_generation_client.cc*P4_SERVER*depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc*12345
c:\projects\breakpad-tools\deps\breakpad\src\common\scoped_ptr.h*P4_SERVER*depot/breakpad/src/common/scoped_ptr.h*12346
c:\projects\breakpad-tools\deps\breakpad\src\common\windows\string_utils-inl.h*P4_SERVER*depot/breakpad/src/common/windows/string_utils-inl.h*12347
c:\program files (x86)\microsoft visual studio\2017\community\vc\tools\msvc\14.13.26128\include\system_error*P4_SERVER*depot/msvc/2017/include/system_error*67890
SRCSRV: end ------------------------------------------------"#;

        let mappings = SourceServerMappings::parse(stream).unwrap();
        let info = mappings.get_info(r"c:\projects\breakpad-tools\deps\breakpad\src\client\windows\crash_generation\crash_generation_client.cc").unwrap();

        assert_eq!(
            info.path,
            "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc"
        );
        assert_eq!(info.revision.as_deref(), Some("12345"));
    }
}
