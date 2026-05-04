//! Tests for PDB SRCSRV functionality
//!
//! These tests verify that source server (SRCSRV) information embedded in PDB files
//! is properly parsed and used to fill in file information. This is commonly used in game
//! development where builds happen on different machines with Perforce.

use symbolic_common::ByteView;
use symbolic_debuginfo::pdb::PdbObject;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture;

#[test]
fn test_pdb_srcsrv_vcs_name() {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb")).unwrap();
    let pdb = PdbObject::parse(&view).unwrap();

    // This PDB file contains Perforce SRCSRV information
    let vcs_name = pdb.debug_session().unwrap().srcsrv_vcs_name();
    assert_eq!(vcs_name, Some("Perforce".to_string()));
}

#[test]
fn test_pdb_file_info() {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb")).unwrap();
    let object = Object::parse(&view).unwrap();

    let session = object.debug_session().unwrap();

    let file = session
        .files()
        .map(|file| file.unwrap())
        .find(|file| {
            // Expected specific path based on the SRCSRV data in the test PDB:
            // c:\projects\breakpad-tools\deps\breakpad\src\client\windows\crash_generation\crash_generation_client.cc
            //   -> depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc (path)
            //   -> 12345 (revision)
            file.srcsrv_path_str().as_deref()
                == Some(
                    "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc",
                )
        })
        .unwrap();

    assert_eq!(file.srcsrv_revision(), Some("12345"));
}

#[test]
fn test_pdb_line_info() {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb")).unwrap();
    let object = Object::parse(&view).unwrap();

    let session = object.debug_session().unwrap();

    let line = session
        .functions()
        .map(|function| function.unwrap())
        .flat_map(|function| function.lines.into_iter())
        .find(|line| {
            // Expected specific path based on the SRCSRV data in the test PDB
            // Path should NOT contain @12345, revision should be separate
            line.file.srcsrv_path_str().as_deref()
                == Some(
                    "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc",
                )
        })
        .unwrap();

    assert_eq!(line.file.srcsrv_revision(), Some("12345"),);
}

#[test]
fn test_pdb_without_srcsrv() {
    let view = ByteView::open(fixture("windows/crash.pdb")).unwrap();
    let pdb = PdbObject::parse(&view).unwrap();

    let vcs_name = pdb.debug_session().unwrap().srcsrv_vcs_name();
    assert_eq!(vcs_name, None);

    let srcsrv_data = pdb.has_source_server_data().unwrap();
    assert!(
        !srcsrv_data,
        "Regular PDB without SRCSRV should return None"
    );
}
