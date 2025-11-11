//! Tests for PDB SRCSRV path remapping functionality
//!
//! These tests verify that source server (SRCSRV) information embedded in PDB files
//! is properly parsed and used to remap file paths. This is commonly used in game
//! development where builds happen on different machines with Perforce.

use symbolic_common::ByteView;
use symbolic_debuginfo::pdb::PdbObject;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

#[test]
fn test_pdb_srcsrv_vcs_name() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let pdb = PdbObject::parse(&view)?;

    // This PDB file contains Perforce SRCSRV information
    let vcs_name = pdb.srcsrv_vcs_name();
    assert_eq!(vcs_name, Some("Perforce".to_string()));

    Ok(())
}

#[test]
fn test_pdb_has_srcsrv_data() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let pdb = PdbObject::parse(&view)?;

    // Verify SRCSRV data exists
    let srcsrv_bytes = pdb
        .source_server_data()?
        .expect("crash_with_srcsrv.pdb should have SRCSRV data");

    assert!(!srcsrv_bytes.is_empty(), "SRCSRV data should not be empty");

    // Verify it's Perforce data
    let srcsrv_str = std::str::from_utf8(&srcsrv_bytes)?;
    assert!(
        srcsrv_str.contains("VERCTRL=") && srcsrv_str.to_lowercase().contains("perforce"),
        "SRCSRV data should be for Perforce version control"
    );

    Ok(())
}

#[test]
fn test_pdb_files_are_remapped() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&view)?;

    // Get the debug session which will parse SRCSRV and remap paths
    let session = object.debug_session()?;

    // Collect all file paths from the PDB
    let mut files: Vec<String> = Vec::new();
    for file_result in session.files() {
        let file_entry = file_result?;
        let path = file_entry.abs_path_str();
        files.push(path);
    }

    // Verify we found some files
    assert!(!files.is_empty(), "PDB should contain file entries");

    // Expected specific path based on the SRCSRV data in the test PDB:
    // c:\projects\breakpad-tools\deps\breakpad\src\client\windows\crash_generation\crash_generation_client.cc
    //   -> depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc@12345
    let expected_path =
        "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc@12345";

    // Verify the exact expected path exists
    assert!(
        files.iter().any(|f| f == expected_path),
        "Expected to find remapped path: {}. Found paths: {:?}",
        expected_path,
        &files[..files.len().min(10)]
    );

    Ok(())
}

#[test]
fn test_pdb_functions_are_remapped() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&view)?;

    // Get the debug session which will parse SRCSRV and remap paths
    let session = object.debug_session()?;

    // Collect all file paths from functions' line info
    let mut files: Vec<String> = Vec::new();
    for func_result in session.functions() {
        let func = func_result?;
        for line in &func.lines {
            let path = line.file.path_str();
            if !files.contains(&path) {
                files.push(path);
            }
        }
    }

    // Verify we found some files
    assert!(!files.is_empty(), "Functions should contain file entries");

    // Expected specific path based on the SRCSRV data in the test PDB
    let expected_path =
        "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc@12345";

    // Verify the exact expected path exists
    assert!(
        files.iter().any(|f| f == expected_path),
        "Expected to find remapped path in functions: {}. Found paths: {:?}",
        expected_path,
        &files[..files.len().min(10)]
    );

    Ok(())
}

#[test]
fn test_pdb_without_srcsrv() -> Result<(), Error> {
    // Test with a regular PDB that doesn't have SRCSRV data
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let pdb = PdbObject::parse(&view)?;

    // This PDB file does not contain SRCSRV information
    let vcs_name = pdb.srcsrv_vcs_name();
    assert_eq!(vcs_name, None);

    // Should return None for PDBs without SRCSRV
    let srcsrv_data = pdb.source_server_data()?;
    assert!(
        srcsrv_data.is_none(),
        "Regular PDB without SRCSRV should return None"
    );

    // Parsing should still work, just without path remapping
    let object = Object::parse(&view)?;
    let session = object.debug_session()?;

    // Should still be able to iterate files (just without remapping)
    let mut file_count = 0;
    for file_result in session.files() {
        file_result?;
        file_count += 1;
        if file_count >= 5 {
            break; // Just verify a few files work
        }
    }

    assert!(
        file_count > 0,
        "Should be able to read files from regular PDB"
    );

    Ok(())
}
