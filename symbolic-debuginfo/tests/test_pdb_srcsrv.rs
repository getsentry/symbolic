use symbolic_common::ByteView;
use symbolic_debuginfo::pdb::PdbObject;
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
fn test_pdb_without_srcsrv() -> Result<(), Error> {
    let view = ByteView::open(fixture("windows/crash.pdb"))?;
    let pdb = PdbObject::parse(&view)?;

    // This PDB file does not contain SRCSRV information
    let vcs_name = pdb.srcsrv_vcs_name();
    assert_eq!(vcs_name, None);

    Ok(())
}
