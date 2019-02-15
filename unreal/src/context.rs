//! Unreal Engine 4 crash context information
#![warn(missing_docs)]

use elementtree::{Element, QName};

#[cfg(feature = "with-serde")]
use serde::Serialize;

use crate::Unreal4Crash;
use crate::Unreal4Error;
use crate::Unreal4FileType;

/// The context data found in the context xml file.
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp)
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "with-serde", derive(Serialize))]
pub struct Unreal4Context {
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub runtime_properties: Option<Unreal4ContextRuntimeProperties>,
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub platform_properties: Option<Unreal4ContextPlatformProperties>,
}

/// RuntimeProperties context element
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L274)
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "with-serde", derive(Serialize))]
pub struct Unreal4ContextRuntimeProperties {
    /// CrashGUID
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_guid: Option<String>,
    /// ProcessId
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub process_id: Option<u32>,
    /// IsInternalBuild
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_internal_build: Option<bool>,
    /// IsSourceDistribution
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_source_distribution: Option<bool>,
    /// IsAssert
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_assert: Option<bool>,
    /// IsEnsure
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_ensure: Option<bool>,
    /// CrashType
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_type: Option<String>,
    /// SecondsSinceStart
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub seconds_since_start: Option<u32>,
    /// GameName
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub game_name: Option<String>,
    /// ExecutableName
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub executable_name: Option<String>,
    /// BuildConfiguration
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub build_configuration: Option<String>,
    /// PlatformName
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub platform_name: Option<String>,
    /// EngineMode
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub engine_mode: Option<String>,
    /// EngineVersion
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub engine_version: Option<String>,
    /// LanguageLCID
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub language_lcid: Option<i32>,
    /// AppDefaultLocale
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub app_default_locate: Option<String>,
    /// BuildVersion
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub build_version: Option<String>,
    /// IsUE4Release
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_ue4_release: Option<bool>,
    /// UserName
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub username: Option<String>,
    /// BaseDir
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub base_dir: Option<String>,
    /// RootDir
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub root_dir: Option<String>,
    /// MachineId
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub machine_id: Option<String>,
    /// LoginId
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub login_id: Option<String>,
    /// EpicAccountId
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub epic_account_id: Option<String>,
    /// CallStack
    /// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L326-L327)
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub legacy_call_stack: Option<String>,
    /// PCallStack
    // [Sopurce](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L329-L330)
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub portable_call_stack: Option<String>,
    /// UserDescription
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub user_description: Option<String>,
    /// ErrorMessage
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub error_message: Option<String>,
    /// CrashReporterMessage
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_reporter_message: Option<String>,
    /// Misc.NumberOfCores
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_number_of_cores: Option<u32>,
    /// Misc.NumberOfCoresIncludingHyperthreads
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_number_of_cores_inc_hyperthread: Option<u32>,
    /// Misc.Is64bitOperatingSystem
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_is_64bit: Option<bool>,
    /// Misc.CPUVendor
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_cpu_vendor: Option<String>,
    /// Misc.CPUBrand
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_cpu_brand: Option<String>,
    /// Misc.PrimaryGPUBrand
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_primary_cpu_brand: Option<String>,
    /// Misc.OSVersionMajor
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_os_version_major: Option<String>,
    /// Misc.OSVersionMinor
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_os_version_minor: Option<String>,
    /// GameStateName
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub game_state_name: Option<String>,
    /// MemoryStats.TotalPhysical
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_physical: Option<u64>,
    /// MemoryStats.TotalVirtual
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_virtual: Option<u64>,
    /// MemoryStats.PageSize
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_page_size: Option<u64>,
    /// MemoryStats.TotalPhysicalGB
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_phsysical_gb: Option<u32>,
    /// TimeOfCrash
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub time_of_crash: Option<u64>,
    /// bAllowToBeContacted
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub allowed_to_be_contacted: Option<bool>,
    /// CrashReportClientVersion
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_reporter_client_version: Option<String>,
    /// Modules
    #[cfg_attr(feature = "with-serde", serde(skip_serializing_if = "Option::is_none"))]
    pub modules: Option<String>,
}

/// Platform specific properties.
/// [Source[(https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L451-L455)
/// [Windows](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/Windows/WindowsPlatformCrashContext.cpp#L39-L44)
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "with-serde", derive(Serialize))]
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

        let root = Element::from_reader(file).map_err(Unreal4Error::InvalidXml)?;

        Ok(Some(Unreal4Context {
            runtime_properties: get_runtime_properties(&root),
            platform_properties: get_platform_properties(&root),
        }))
    }
}

fn get_runtime_properties(root: &Element) -> Option<Unreal4ContextRuntimeProperties> {
    let list = root.find("RuntimeProperties")?;

    let mut rv = Unreal4ContextRuntimeProperties::default();

    fn get_text_or_none(elm: &Element) -> Option<String> {
        let text = elm.text();
        if text != "" {
            Some(text.to_string())
        } else {
            None
        }
    }

    for child in list.children() {
        let tag = child.tag();

        // We don't expect an XML with namespace here
        if tag.ns().is_some() {
            continue;
        }
        match tag.name() {
            "CrashGUID" => rv.crash_guid = get_text_or_none(&child),
            "ProcessId" => rv.process_id = child.text().parse::<u32>().ok(),
            "IsInternalBuild" => rv.is_internal_build = child.text().parse::<bool>().ok(),
            "IsSourceDistribution" => rv.is_source_distribution = child.text().parse::<bool>().ok(),
            "IsAssert" => rv.is_assert = child.text().parse::<bool>().ok(),
            "IsEnsure" => rv.is_ensure = child.text().parse::<bool>().ok(),
            "CrashType" => rv.crash_type = get_text_or_none(&child),
            "SecondsSinceStart" => rv.seconds_since_start = child.text().parse::<u32>().ok(),
            "GameName" => rv.game_name = get_text_or_none(&child),
            "ExecutableName" => rv.executable_name = get_text_or_none(&child),
            "BuildConfiguration" => rv.build_configuration = get_text_or_none(&child),
            "PlatformName" => rv.platform_name = get_text_or_none(&child),
            "EngineMode" => rv.engine_mode = get_text_or_none(&child),
            "EngineVersion" => rv.engine_version = get_text_or_none(&child),
            "LanguageLCID" => rv.language_lcid = child.text().parse::<i32>().ok(),
            "AppDefaultLocale" => rv.app_default_locate = get_text_or_none(&child),
            "BuildVersion" => rv.build_version = get_text_or_none(&child),
            "IsUE4Release" => rv.is_ue4_release = child.text().parse::<bool>().ok(),
            "UserName" => rv.username = get_text_or_none(&child),
            "BaseDir" => rv.base_dir = get_text_or_none(&child),
            "RootDir" => rv.root_dir = get_text_or_none(&child),
            "MachineId" => rv.machine_id = get_text_or_none(&child),
            "LoginId" => rv.login_id = get_text_or_none(&child),
            "EpicAccountId" => rv.epic_account_id = get_text_or_none(&child),
            "CallStack" => rv.legacy_call_stack = get_text_or_none(&child),
            "PCallStack" => rv.portable_call_stack = get_text_or_none(&child),
            "UserDescription" => rv.user_description = get_text_or_none(&child),
            "ErrorMessage" => rv.error_message = get_text_or_none(&child),
            "CrashReporterMessage" => rv.crash_reporter_message = get_text_or_none(&child),
            "Misc.NumberOfCores" => rv.misc_number_of_cores = child.text().parse::<u32>().ok(),
            "Misc.NumberOfCoresIncludingHyperthreads" => {
                rv.misc_number_of_cores_inc_hyperthread = child.text().parse::<u32>().ok()
            }
            "Misc.Is64bitOperatingSystem" => rv.misc_is_64bit = child.text().parse::<bool>().ok(),
            "Misc.CPUVendor" => rv.misc_cpu_vendor = get_text_or_none(&child),
            "Misc.CPUBrand" => rv.misc_cpu_brand = get_text_or_none(&child),
            "Misc.PrimaryGPUBrand" => rv.misc_primary_cpu_brand = get_text_or_none(&child),
            "Misc.OSVersionMajor" => rv.misc_os_version_major = get_text_or_none(&child),
            "Misc.OSVersionMinor" => rv.misc_os_version_minor = get_text_or_none(&child),
            "GameStateName" => rv.game_state_name = get_text_or_none(&child),
            "MemoryStats.TotalPhysical" => {
                rv.memory_stats_total_physical = child.text().parse::<u64>().ok()
            }
            "MemoryStats.TotalVirtual" => {
                rv.memory_stats_total_virtual = child.text().parse::<u64>().ok()
            }
            "MemoryStats.PageSize" => rv.memory_stats_page_size = child.text().parse::<u64>().ok(),
            "MemoryStats.TotalPhysicalGB" => {
                rv.memory_stats_total_phsysical_gb = child.text().parse::<u32>().ok()
            }
            "TimeOfCrash" => rv.time_of_crash = child.text().parse::<u64>().ok(),
            "bAllowToBeContacted" => rv.allowed_to_be_contacted = child.text().parse::<bool>().ok(),
            "CrashReportClientVersion" => {
                rv.crash_reporter_client_version = get_text_or_none(&child)
            }
            "Modules" => rv.modules = get_text_or_none(&child),
            _ => {}
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
            match child.text().parse::<bool>() {
                Ok(true) => rv.is_windows = Some(true),
                Ok(false) => rv.is_windows = Some(false),
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

macro_rules! test_unreal_contect {
    ($xml_parent:expr, $func_name:expr, $name:ident, $xml_elm:expr, $expect:expr, $(,)*) => {
        #[cfg(test)]
        mod $name {
            use super::*;

            #[test]
            fn test_some() {
                #[rustfmt::skip]
                let xml = concat!("<FGenericCrashContext><", $xml_parent, "><", $xml_elm, ">", $expect, "</", $xml_elm, "></", $xml_parent, "></FGenericCrashContext>");
                let root = Element::from_reader(xml.as_bytes()).unwrap();
                let runtime_properties = $func_name(&root).expect("RuntimeProperties exists");
                assert_eq!(
                    $expect,
                    runtime_properties.$name.expect("missing property value")
                );
            }

            #[test]
            fn test_none() {
                #[rustfmt::skip]
                let xml = concat!("<FGenericCrashContext><", $xml_parent, "><", $xml_elm, "></", $xml_elm, "></", $xml_parent, "></FGenericCrashContext>");
                let root = Element::from_reader(xml.as_bytes()).unwrap();
                let runtime_properties = $func_name(&root).expect("RuntimeProperties exists");
                assert!(runtime_properties.$name.is_none());
            }
        }
    };
}

macro_rules! test_unreal_runtime_properties {
    ($name:ident, $xml_elm:expr, $expect:expr $(,)*) => {
        test_unreal_contect!(
            "RuntimeProperties",
            get_runtime_properties,
            $name,
            $xml_elm,
            $expect,
        );
    };
}

macro_rules! test_unreal_platform_properties {
    ($name:ident, $xml_elm:expr, $expect:expr $(,)*) => {
        test_unreal_contect!(
            "PlatformProperties",
            get_platform_properties,
            $name,
            $xml_elm,
            $expect,
        );
    };
}

test_unreal_runtime_properties!(
    crash_guid,
    "CrashGUID",
    "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000",
);

test_unreal_runtime_properties!(process_id, "ProcessId", 2576);
test_unreal_runtime_properties!(is_internal_build, "IsInternalBuild", true);
test_unreal_runtime_properties!(is_source_distribution, "IsSourceDistribution", true);
test_unreal_runtime_properties!(is_ensure, "IsEnsure", true);
test_unreal_runtime_properties!(is_assert, "IsAssert", true);
test_unreal_runtime_properties!(crash_type, "CrashType", "Crash");
test_unreal_runtime_properties!(seconds_since_start, "SecondsSinceStart", 4);
test_unreal_runtime_properties!(game_name, "GameName", "UE4-YetAnother");
test_unreal_runtime_properties!(executable_name, "ExecutableName", "YetAnother");
test_unreal_runtime_properties!(build_configuration, "BuildConfiguration", "Development");
test_unreal_runtime_properties!(platform_name, "PlatformName", "WindowsNoEditor");
test_unreal_runtime_properties!(engine_mode, "EngineMode", "Game");
test_unreal_runtime_properties!(engine_version, "EngineVersion", "Game");
test_unreal_runtime_properties!(language_lcid, "LanguageLCID", 1033);
test_unreal_runtime_properties!(app_default_locate, "AppDefaultLocale", "en-US");
test_unreal_runtime_properties!(
    build_version,
    "BuildVersion",
    "++UE4+Release-4.20-CL-4369336"
);
test_unreal_runtime_properties!(is_ue4_release, "IsUE4Release", true);
test_unreal_runtime_properties!(username, "UserName", "bruno");
test_unreal_runtime_properties!(
    base_dir,
    "BaseDir",
    "//Mac/Home/Desktop/WindowsNoEditor/YetAnother/Binaries/Win64/"
);
test_unreal_runtime_properties!(root_dir, "RootDir", "/Mac/Home/Desktop/WindowsNoEditor/");
test_unreal_runtime_properties!(machine_id, "MachineId", "9776D4844CC893F55395DBBEFB0EB6D7");
test_unreal_runtime_properties!(login_id, "LoginId", "9776d4844cc893f55395dbbefb0eb6d7");
test_unreal_runtime_properties!(epic_account_id, "EpicAccountId", "epic acc id");
test_unreal_runtime_properties!(
    legacy_call_stack,
    "CallStack",
    r#"YetAnother!AActor::IsPendingKillPending()
YetAnother!__scrt_common_main_seh() [f:\dd\vctools\crt\vcstartup\src\startup\exe_common.inl:288]
kernel32
ntdll"#
);
test_unreal_runtime_properties!(portable_call_stack, "PCallStack", "YetAnother 0x0000000025ca0000 + 703394 YetAnother 0x0000000025ca0000 + 281f2ee YetAnother 0x0000000025ca0000 + 2a26dd3 YetAnother 0x0000000025ca0000 + 2a4f984 YetAnother 0x0000000025ca0000 + 355e77e YetAnother 0x0000000025ca0000 + 3576186 YetAnother 0x0000000025ca0000 + 8acc56 YetAnother 0x0000000025ca0000 + 8acf00 YetAnother 0x0000000025ca0000 + 35c121d YetAnother 0x0000000025ca0000 + 35cfb58 YetAnother 0x0000000025ca0000 + 2eb082f YetAnother 0x0000000025ca0000 + 2eb984f YetAnother 0x0000000025ca0000 + 2d1cd39 YetAnother 0x0000000025ca0000 + 325258 YetAnother 0x0000000025ca0000 + 334e4c YetAnother 0x0000000025ca0000 + 334eaa YetAnother 0x0000000025ca0000 + 3429e6 YetAnother 0x0000000025ca0000 + 44e73c6 KERNEL32 0x000000000fd40000 + 13034 ntdll 0x0000000010060000 + 71471");
test_unreal_runtime_properties!(
    error_message,
    "ErrorMessage",
    "Access violation - code c0000005 (first/second chance not available)"
);
test_unreal_runtime_properties!(crash_reporter_message, "CrashReporterMessage", "message");
test_unreal_runtime_properties!(misc_number_of_cores, "Misc.NumberOfCores", 6);
test_unreal_runtime_properties!(
    misc_number_of_cores_inc_hyperthread,
    "Misc.NumberOfCoresIncludingHyperthreads",
    6
);
test_unreal_runtime_properties!(misc_is_64bit, "Misc.Is64bitOperatingSystem", true);
test_unreal_runtime_properties!(misc_cpu_vendor, "Misc.CPUVendor", "GenuineIntel");
test_unreal_runtime_properties!(
    misc_cpu_brand,
    "Misc.CPUBrand",
    "Intel(R) Core(TM) i7-7920HQ CPU @ 3.10GHz"
);
test_unreal_runtime_properties!(
    misc_primary_cpu_brand,
    "Misc.PrimaryGPUBrand",
    "Parallels Display Adapter (WDDM)"
);
test_unreal_runtime_properties!(misc_os_version_major, "Misc.OSVersionMajor", "Windows 10");
test_unreal_runtime_properties!(
    misc_os_version_minor,
    "Misc.OSVersionMinor",
    "some minor version"
);
test_unreal_runtime_properties!(game_state_name, "GameStateName", "game state");
test_unreal_runtime_properties!(
    memory_stats_total_physical,
    "MemoryStats.TotalPhysical",
    6_896_832_512,
);
test_unreal_runtime_properties!(
    memory_stats_total_virtual,
    "MemoryStats.TotalVirtual",
    140_737_488_224_256,
);
test_unreal_runtime_properties!(memory_stats_page_size, "MemoryStats.PageSize", 4096);
test_unreal_runtime_properties!(
    memory_stats_total_phsysical_gb,
    "MemoryStats.TotalPhysicalGB",
    7
);
test_unreal_runtime_properties!(time_of_crash, "TimeOfCrash", 636_783_195_289_630_000,);
test_unreal_runtime_properties!(allowed_to_be_contacted, "bAllowToBeContacted", true);
test_unreal_runtime_properties!(
    crash_reporter_client_version,
    "CrashReportClientVersion",
    "1.0",
);
test_unreal_runtime_properties!(modules, "Modules", r#"\\Mac\Home\Desktop\WindowsNoEditor\YetAnother\Binaries\Win64\YetAnother.exe
\\Mac\Home\Desktop\WindowsNoEditor\Engine\Binaries\ThirdParty\Vorbis\Win64\VS2015\libvorbis_64.dll"#);

test_unreal_platform_properties!(is_windows, "PlatformIsRunningWindows", true);
test_unreal_platform_properties!(callback_result, "PlatformCallbackResult", 123);
