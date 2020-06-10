use std::fs::File;
use std::io::Read;

use symbolic_testutils::fixture;
use symbolic_unreal::{Unreal4Crash, Unreal4Error, Unreal4FileType};

fn get_unreal_crash() -> Result<Unreal4Crash, Unreal4Error> {
    let mut file = File::open(fixture("unreal/unreal_crash")).expect("example file opens");
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content).expect("fixture file");
    Unreal4Crash::parse(&file_content)
}

fn get_unreal_apple_crash() -> Result<Unreal4Crash, Unreal4Error> {
    let mut file = File::open(fixture("unreal/unreal_crash_apple")).expect("example file opens");
    let mut file_content = Vec::new();
    file.read_to_end(&mut file_content).expect("fixture file");
    Unreal4Crash::parse(&file_content)
}

#[test]
fn test_load_unreal_crash() {
    get_unreal_crash().expect("crash file loaded");
}

#[test]
fn test_get_minidump_slice() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let minidump_file = ue4_crash
        .file_by_type(Unreal4FileType::Minidump)
        .expect("expected minidump file exists");

    assert_eq!(minidump_file.data().len(), 410_700);

    let crash_file = ue4_crash
        .native_crash()
        .expect("expected native crash exists");

    assert_eq!(minidump_file.index(), crash_file.index());

    assert!(ue4_crash
        .file_by_type(Unreal4FileType::AppleCrashReport)
        .is_none());
}

#[test]
fn test_get_apple_crash_report() {
    let ue4_crash = get_unreal_apple_crash().expect("test crash file loads");

    let apple_file = ue4_crash
        .file_by_type(Unreal4FileType::AppleCrashReport)
        .expect("expected apple crash report file exists");

    assert_eq!(apple_file.data().len(), 91_392);

    let crash_file = ue4_crash
        .native_crash()
        .expect("expected native crash exists");

    assert_eq!(apple_file.index(), crash_file.index());

    assert!(ue4_crash.file_by_type(Unreal4FileType::Minidump).is_none());
}

#[test]
fn test_contexts_runtime_properties() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let ue4_context = ue4_crash
        .context()
        .expect("no errors parsing the context file")
        .expect("context file exists in sample crash");

    let runtime_properties = ue4_context
        .runtime_properties
        .expect("runtime properties exist within sample crash");

    assert_eq!(
        "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000",
        runtime_properties.crash_guid.as_ref().expect("crash guid")
    );

    insta::assert_yaml_snapshot!(&runtime_properties);
}

#[test]
fn test_contexts_platform_properties() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");

    let ue4_context = ue4_crash
        .context()
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
    assert_eq!(ue4_crash.files().size_hint(), (4, Some(4)));

    assert_eq!(
        ue4_crash.file_by_index(0).expect("File exists").name(),
        "CrashContext.runtime-xml"
    );
    assert_eq!(
        ue4_crash.file_by_index(1).expect("File exists").name(),
        "CrashReportClient.ini"
    );
    assert_eq!(
        ue4_crash.file_by_index(2).expect("File exists").name(),
        "MyProject.log"
    );
    assert_eq!(
        ue4_crash.file_by_index(3).expect("File exists").name(),
        "UE4Minidump.dmp"
    );

    let context_file = ue4_crash.file_by_index(0).expect("xml file in pos 0");
    let xml = context_file.data();

    assert_eq!(xml[0] as char, '<');
    // there are two line breaks after closing tag:
    assert_eq!(xml[xml.len() - 3] as char, '>');
}

#[test]
fn test_get_logs() {
    let ue4_crash = get_unreal_crash().expect("test crash file loads");
    let limit = 100;
    let logs = ue4_crash.logs(limit).expect("log file");

    assert_eq!(logs.len(), limit);
    assert_eq!(
        logs[1].timestamp.expect("timestamp").to_rfc3339(),
        "2018-10-29T16:56:37+00:00"
    );
    assert_eq!(
        logs[0].component.as_ref().expect("component"),
        "LogD3D11RHI"
    );
    assert_eq!(logs[0].message, "Chosen D3D11 Adapter: 0");

    assert_eq!(
        logs[99].timestamp.expect("timestamp").to_rfc3339(),
        "2018-10-29T16:56:38+00:00"
    );
    assert_eq!(
        logs[99].component.as_ref().expect("component"),
        "LogWindows"
    );
    assert_eq!(
        logs[99].message,
        "Windows GetLastError: The operation completed successfully. (0)"
    );
}
