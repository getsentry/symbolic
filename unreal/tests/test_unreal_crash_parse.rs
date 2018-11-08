extern crate bytes;
extern crate failure;
extern crate symbolic_testutils;
extern crate symbolic_unreal;

use std::fs::File;
use std::io::Read;

use symbolic_testutils::fixture_path;
use symbolic_unreal::Unreal4Crash;

#[test]
fn parse_unreal_crash() {
    let mut file = File::open(fixture_path("unreal/unreal_crash")).expect("example file opens");
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content).expect("fixture file");

    let ue4_crash = Unreal4Crash::from_bytes(&file_content).expect("crash file loaded");

    let minidump_bytes = ue4_crash
        .get_minidump_bytes()
        .expect("expected Minidump file read without errors")
        .expect("expected Minidump file bytes exists");

    assert_eq!(minidump_bytes.len(), 410_700);

    assert_eq!(ue4_crash.file_count(), 4);
    assert_eq!(ue4_crash.files().count(), 4);

    assert_eq!(
        ue4_crash.file_by_index(0).expect("File exists").file_name,
        "CrashContext.runtime-xml"
    );
    assert_eq!(
        ue4_crash.file_by_index(1).expect("File exists").file_name,
        "CrashReportClient.ini"
    );
    assert_eq!(
        ue4_crash.file_by_index(2).expect("File exists").file_name,
        "MyProject.log"
    );
    assert_eq!(
        ue4_crash.file_by_index(3).expect("File exists").file_name,
        "UE4Minidump.dmp"
    );

    let xml = ue4_crash
        .get_file_content(ue4_crash.file_by_index(0).expect("xml file in pos 0"))
        .expect("contents of xml file");

    assert_eq!(xml[0] as char, '<');
    // there are two line breaks after closing tag:
    assert_eq!(xml[xml.len() - 3] as char, '>');
}
