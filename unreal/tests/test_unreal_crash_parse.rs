extern crate bytes;
extern crate failure;
extern crate symbolic_testutils;
extern crate symbolic_unreal;

use std::fs::File;
use std::io::Read;

use symbolic_testutils::fixture_path;
use symbolic_unreal::{Unreal4Crash, Unreal4Error};

fn get_unreal_crash() -> Result<Unreal4Crash, Unreal4Error> {
    let mut file = File::open(fixture_path("unreal/unreal_crash")).expect("example file opens");
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content).expect("fixture file");
    Unreal4Crash::from_slice(&file_content)
}

#[test]
fn test_load_unreal_crash() {
    get_unreal_crash().expect("crash file loaded");
}

#[test]
fn test_get_minidump_slice() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let minidump_bytes = ue4_crash
        .get_minidump_slice()
        .expect("expected Minidump file read without errors")
        .expect("expected Minidump file bytes exists");

    assert_eq!(minidump_bytes.len(), 410_700);
}

#[test]
fn test_contexts_runtime_properties() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let ue4_context = ue4_crash
        .get_context()
        .expect("no errors parsing the context file")
        .expect("context file exists in sample crash");

    let runtime_properties = ue4_context
        .runtime_properties
        .expect("runtime properties exist within sample crash");

    assert_eq!(
        "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000",
        runtime_properties.crash_guid.expect("crash guid")
    );
}

#[test]
fn test_contexts_platform_properties() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let ue4_context = ue4_crash
        .get_context()
        .expect("no errors parsing the context file")
        .expect("context file exists in sample crash");

    let platform_properties = ue4_context
        .platform_properties
        .expect("platform properties exist within sample crash");

    assert_eq!(
        platform_properties
            .is_windows
            .expect("sample contains value as 1 for true"),
        true
    );

    assert_eq!(
        platform_properties
            .callback_result
            .expect("sample contains value 0"),
        0
    );
}

#[test]
fn test_files_api() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

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
        .get_file_contents(ue4_crash.file_by_index(0).expect("xml file in pos 0"))
        .expect("contents of xml file");

    assert_eq!(xml[0] as char, '<');
    // there are two line breaks after closing tag:
    assert_eq!(xml[xml.len() - 3] as char, '>');
}
