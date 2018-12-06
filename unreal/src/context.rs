//! Unreal Engine 4 crash context information
#![warn(missing_docs)]

extern crate failure;

use Unreal4Crash;
use Unreal4Error;
use Unreal4FileType;

use elementtree::{Element, QName};

/// The context data found in the context xml file.
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp)
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Unreal4Context {
    pub runtime_properties: Option<Unreal4ContextRuntimeProperties>,
    pub platform_properties: Option<Unreal4ContextPlatformProperties>,
}

/// RuntimeProperties context element
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L274)
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Unreal4ContextRuntimeProperties {
    /// CrashGUID
    pub crash_guid: Option<String>,
    /// ProcessId
    pub process_id: Option<u32>,
    /// IsInternalBuild
    pub is_internal_build: Option<bool>,
    /// IsSourceDistribution
    pub is_source_distribution: Option<bool>,
    /// IsEnsure
    pub is_assert: Option<bool>,
    /// IsAssert
    pub is_ensure: Option<bool>,
    /// CrashType
    pub crash_type: Option<String>,
    /// SecondsSinceStart
    pub seconds_since_start: Option<u32>,
    /// GameName
    pub game_name: Option<String>,
    /// ExecutableName
    pub executable_name: Option<String>,
    /// BuildConfiguration
    pub build_configuration: Option<String>,
    /// PlatformName
    pub platform_name: Option<String>,
    /// EngineMode
    pub engine_mode: Option<String>,
    /// EngineVersion
    pub engine_version: Option<String>,
    /// LanguageLCID
    pub language_lcid: Option<i32>,
    /// AppDefaultLocale
    pub app_default_locate: Option<String>,
    /// BuildVersion
    pub build_version: Option<String>,
    /// IsUE4Release
    pub is_ue4_release: Option<bool>,
    /// UserName
    pub username: Option<String>,
    /// BaseDir
    pub base_dir: Option<String>,
    /// RootDir
    pub root_dir: Option<String>,
    /// MachineId
    pub machine_id: Option<String>,
    /// LoginId
    pub login_id: Option<String>,
    /// EpicAccountId
    pub epic_account_id: Option<String>,
    /// CallStack
    /// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L326-L327)
    pub legacy_call_stack: Option<String>,
    /// PCallStack
    // [Sopurce](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L329-L330)
    pub portable_call_stack: Option<String>,
    /// ErrorMessage
    pub error_message: Option<String>,
    /// CrashReporterMessage
    pub crash_reporter_message: Option<String>,
    /// Misc.NumberOfCores
    pub misc_number_of_cores: Option<u32>,
    /// Misc.NumberOfCoresIncludingHyperthreads
    pub misc_number_of_cores_inc_hyperthread: Option<u32>,
    /// Misc.Is64bitOperatingSystem
    pub misc_is_64bit: Option<bool>,
    /// Misc.CPUVendor
    pub misc_cpu_vendor: Option<String>,
    /// Misc.CPUBrand
    pub misc_cpu_brand: Option<String>,
    /// Misc.PrimaryGPUBrand
    pub misc_primary_cpu_brand: Option<String>,
    /// Misc.OSVersionMajor
    pub misc_os_version_major: Option<String>,
    /// Misc.OSVersionMinor
    pub misc_os_version_minor: Option<String>,
    /// GameStateName
    pub game_state_name: Option<String>,
    /// MemoryStats.TotalPhysical
    pub memory_stats_total_physical: Option<u64>,
    /// MemoryStats.TotalVirtual
    pub memory_stats_total_virtual: Option<u64>,
    /// MemoryStats.PageSize
    pub memory_stats_page_size: Option<u64>,
    /// MemoryStats.TotalPhysicalGB
    pub memory_stats_total_phsysical_gb: Option<u32>,
    /// TimeOfCrash
    pub time_of_crash: Option<u64>,
    /// bAllowToBeContacted
    pub allowed_to_be_contacted: Option<bool>,
    /// CrashReportClientVersion
    pub crash_reporter_client_version: Option<String>,
    /// Modules
    pub modules: Option<String>,
}

/// Platform specific properties.
/// [Source[(https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L451-L455)
/// [Windows](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/Windows/WindowsPlatformCrashContext.cpp#L39-L44)
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Unreal4ContextPlatformProperties {
    /// Whether the crash happened on a Windows device.
    pub is_windows: Option<bool>,
    /// Platform-specific UE4 Core value.
    pub callback_result: Option<i32>,
}

impl Unreal4Context {
    pub(crate) fn from_crash(crash: &Unreal4Crash) -> Result<Option<Self>, Unreal4Error> {
        let file = match crash.get_file_slice(Unreal4FileType::Context)? {
            Some(f) => f,
            None => return Ok(None),
        };

        let root = Element::from_reader(file).map_err(|e| Unreal4Error::InvalidXml(e))?;

        Ok(Some(Unreal4Context {
            runtime_properties: get_runtime_properties(&root),
            platform_properties: get_platform_properties(&root),
        }))
    }
}

fn get_runtime_properties(root: &Element) -> Option<Unreal4ContextRuntimeProperties> {
    let list = root.find("RuntimeProperties")?;

    let mut rv = Unreal4ContextRuntimeProperties::default();

    for child in list.children() {
        if child.tag() == &QName::from("CrashGUID") {
            let text = child.text();
            if text != "" {
                rv.crash_guid = Some(child.text().to_string());
            }
        } else if child.tag() == &QName::from("ProcessId") {
            rv.process_id = child.text().parse::<u32>().ok();
        }
    }

    Some(rv)
}

fn get_platform_properties(root: &Element) -> Option<Unreal4ContextPlatformProperties> {
    let list = root.find("PlatformProperties")?;

    let mut rv = Unreal4ContextPlatformProperties::default();

    for child in list.children() {
        if child.tag() == &QName::from("PlatformIsRunningWindows") {
            match child.text().parse::<u32>() {
                Ok(1) => rv.is_windows = Some(true),
                Ok(0) => rv.is_windows = Some(false),
                Ok(_) => {}
                Err(_) => {}
            }
        } else if child.tag() == &QName::from("PlatformCallbackResult") {
            rv.callback_result = child.text().parse::<i32>().ok();
        }
    }

    Some(rv)
}

#[allow(dead_code)]
const ONLY_ROOT_NODE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FGenericCrashContext>
</FGenericCrashContext>
"#;

#[allow(dead_code)]
const ONLY_ROOT_AND_CHILD_NODES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FGenericCrashContext>
    <RuntimeProperties>
    </RuntimeProperties>
    <PlatformProperties>
    </PlatformProperties>
</FGenericCrashContext>
"#;

#[test]
fn test_get_runtime_properties_missing_element() {
    let root = Element::from_reader(ONLY_ROOT_NODE.as_bytes()).unwrap();
    assert!(get_runtime_properties(&root).is_none());
}

#[test]
fn test_get_platform_properties_missing_element() {
    let root = Element::from_reader(ONLY_ROOT_NODE.as_bytes()).unwrap();
    assert!(get_platform_properties(&root).is_none());
}

#[test]
fn test_get_runtime_properties_no_children() {
    let root = Element::from_reader(ONLY_ROOT_AND_CHILD_NODES.as_bytes()).unwrap();
    let actual = get_runtime_properties(&root).expect("default struct");
    assert_eq!(Unreal4ContextRuntimeProperties::default(), actual)
}

#[test]
fn test_get_platform_properties_no_children() {
    let root = Element::from_reader(ONLY_ROOT_AND_CHILD_NODES.as_bytes()).unwrap();
    let actual = get_platform_properties(&root).expect("default struct");
    assert_eq!(Unreal4ContextPlatformProperties::default(), actual)
}

#[test]
fn test_get_runtime_properties() {
    let root = Element::from_reader(
        r#"<FGenericCrashContext><RuntimeProperties><CrashGUID>UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000</CrashGUID></RuntimeProperties></FGenericCrashContext>"#.as_bytes(),
    ).unwrap();
    let runtime_properties = get_runtime_properties(&root).expect("RuntimeProperties exists");
    assert_eq!(
        "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000",
        runtime_properties.crash_guid.expect("crash guid")
    );
}

#[test]
fn test_get_platform_properties() {
    let root = Element::from_reader(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<FGenericCrashContext>
	<PlatformProperties>
		<PlatformIsRunningWindows>0</PlatformIsRunningWindows>
	</PlatformProperties>
</FGenericCrashContext>
"#.as_bytes(),
    ).unwrap();

    let platform_properties = get_platform_properties(&root).expect("PlatformProperties exists");
    assert!(!platform_properties.is_windows.expect("is windows"));
}
