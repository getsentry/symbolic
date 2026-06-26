//! Unreal Engine 4 crash context information
#![warn(missing_docs)]

use std::collections::BTreeMap;

use quick_xml::{escape::resolve_xml_entity, events::BytesStart};
#[cfg(test)]
use similar_asserts::assert_eq;

use crate::error::Unreal4Error;

/// RuntimeProperties context element.
///
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L274)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Unreal4ContextRuntimeProperties {
    /// CrashGUID
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_guid: Option<String>,
    /// ProcessId
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub process_id: Option<u32>,
    /// IsInternalBuild
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_internal_build: Option<bool>,
    /// IsSourceDistribution
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_source_distribution: Option<bool>,
    /// IsAssert
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_assert: Option<bool>,
    /// IsEnsure
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_ensure: Option<bool>,
    /// CrashType
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_type: Option<String>,
    /// SecondsSinceStart
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub seconds_since_start: Option<u32>,
    /// GameName
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub game_name: Option<String>,
    /// ExecutableName
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub executable_name: Option<String>,
    /// BuildConfiguration
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub build_configuration: Option<String>,
    /// PlatformName
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub platform_name: Option<String>,
    /// EngineMode
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub engine_mode: Option<String>,
    /// EngineVersion
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub engine_version: Option<String>,
    /// LanguageLCID
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub language_lcid: Option<i32>,
    /// AppDefaultLocale
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub app_default_locate: Option<String>,
    /// BuildVersion
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub build_version: Option<String>,
    /// IsUE4Release
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub is_ue4_release: Option<bool>,
    /// UserName
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub username: Option<String>,
    /// BaseDir
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub base_dir: Option<String>,
    /// RootDir
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub root_dir: Option<String>,
    /// MachineId
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub machine_id: Option<String>,
    /// LoginId
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub login_id: Option<String>,
    /// EpicAccountId
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub epic_account_id: Option<String>,
    /// CallStack
    /// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L326-L327)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub legacy_call_stack: Option<String>,
    /// PCallStack
    // [Sopurce](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L329-L330)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub portable_call_stack: Option<String>,
    /// UserDescription
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub user_description: Option<String>,
    /// ErrorMessage
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub error_message: Option<String>,
    /// CrashReporterMessage
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_reporter_message: Option<String>,
    /// Misc.NumberOfCores
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_number_of_cores: Option<u32>,
    /// Misc.NumberOfCoresIncludingHyperthreads
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_number_of_cores_inc_hyperthread: Option<u32>,
    /// Misc.Is64bitOperatingSystem
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_is_64bit: Option<bool>,
    /// Misc.CPUVendor
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_cpu_vendor: Option<String>,
    /// Misc.CPUBrand
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_cpu_brand: Option<String>,
    /// Misc.PrimaryGPUBrand
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_primary_gpu_brand: Option<String>,
    /// Misc.OSVersionMajor
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_os_version_major: Option<String>,
    /// Misc.OSVersionMinor
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub misc_os_version_minor: Option<String>,
    /// GameStateName
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub game_state_name: Option<String>,
    /// MemoryStats.TotalPhysical
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_physical: Option<u64>,
    /// MemoryStats.TotalVirtual
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_virtual: Option<u64>,
    /// MemoryStats.PageSize
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_page_size: Option<u64>,
    /// MemoryStats.TotalPhysicalGB
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub memory_stats_total_phsysical_gb: Option<u32>,
    /// TimeOfCrash
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub time_of_crash: Option<u64>,
    /// bAllowToBeContacted
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub allowed_to_be_contacted: Option<bool>,
    /// CrashReportClientVersion
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub crash_reporter_client_version: Option<String>,
    /// Modules
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub modules: Option<String>,
    /// Custom attributes
    pub custom: BTreeMap<String, String>,
}

enum NodeState {
    None,
    Opened { node_depth: i32 },
    Empty,
}

// A helper to keep track of reader state as we move through the XML.
struct XMLReader<'a> {
    reader: quick_xml::Reader<&'a [u8]>,
    node_state: NodeState,
}

impl<'a> XMLReader<'a> {
    fn new(reader: quick_xml::Reader<&'a [u8]>) -> Self {
        Self {
            reader,
            node_state: NodeState::None,
        }
    }

    /// Moves the reader to the specified tag if it exists.  Returns true iff
    /// the tag exists.
    fn find_tag(&mut self, name: &str) -> Result<bool, quick_xml::Error> {
        loop {
            match self.reader.read_event()? {
                quick_xml::events::Event::Eof => break,

                quick_xml::events::Event::Start(bytes_start) => {
                    if bytes_start.name().as_ref() == name.as_bytes() {
                        self.node_state = NodeState::Opened { node_depth: 0 };
                        return Ok(true);
                    }
                }
                quick_xml::events::Event::Empty(bytes_start) => {
                    if bytes_start.name().as_ref() == name.as_bytes() {
                        self.node_state = NodeState::Empty;
                        return Ok(true);
                    }
                }

                _ => {}
            };
        }

        self.node_state = NodeState::None;
        Ok(false)
    }

    /// Returns the text value, parsed, from the node at which the reader is
    /// located.  If there is no such node or value, or the parse fails, this will
    /// return None.  A best-effort is made to deal with text spread between xml nodes.
    /// XML entities are resolved (so '&qout;' is resolved to '"', and the like.)
    fn value<T: std::str::FromStr>(&mut self) -> Result<Option<T>, quick_xml::Error> {
        let NodeState::Opened { node_depth } = &mut self.node_state else {
            return Ok(None);
        };

        let mut val = String::new();
        let text_depth = *node_depth;
        loop {
            match self.reader.read_event()? {
                quick_xml::events::Event::Text(bytes_text) => {
                    if text_depth == *node_depth {
                        val += &bytes_text.decode().unwrap();
                    }
                }

                quick_xml::events::Event::GeneralRef(bytes_text) => {
                    if text_depth == *node_depth {
                        val += resolve_xml_entity(&bytes_text.decode().unwrap()).unwrap();
                    }
                }

                quick_xml::events::Event::End(_) => {
                    *node_depth -= 1;
                }

                // It could be the case that we have text interleaved with nodes, so
                // try to be graceful when handling.
                quick_xml::events::Event::Start(_) => {
                    *node_depth += 1;
                }

                _ => {}
            };

            if *node_depth < text_depth {
                break;
            }
        }

        if val.len() > 0 {
            Ok(val.parse().ok())
        } else {
            Ok(None)
        }
    }

    /// Moves to the next child of this node, returning None if we exhaust the children,
    /// otherwise returning the bytes of the start of the tag.
    fn next_child(&mut self) -> Result<Option<BytesStart<'_>>, quick_xml::Error> {
        let NodeState::Opened { node_depth } = &mut self.node_state else {
            return Ok(None);
        };

        let maybe_bytes = loop {
            let maybe_bytes = match self.reader.read_event()? {
                quick_xml::events::Event::Start(bytes_start) => {
                    *node_depth += 1;
                    if *node_depth == 1 {
                        Some(bytes_start)
                    } else {
                        None
                    }
                }
                quick_xml::events::Event::End(_) => {
                    *node_depth -= 1;
                    None
                }
                quick_xml::events::Event::Empty(bytes_start) => {
                    if *node_depth == 0 {
                        Some(bytes_start)
                    } else {
                        None
                    }
                }

                quick_xml::events::Event::Eof => {
                    *node_depth = -1;
                    break None;
                }

                _ => None,
            };

            if maybe_bytes.is_some() {
                break maybe_bytes;
            }

            if *node_depth < 0 {
                self.node_state = NodeState::None;
                break None;
            }
        };

        Ok(maybe_bytes)
    }
}

impl Unreal4ContextRuntimeProperties {
    fn from_xml(root: &[u8]) -> Result<Option<Self>, quick_xml::Error> {
        let r = quick_xml::Reader::from_reader(root);
        let mut r = XMLReader::new(r);

        let mut rv = Unreal4ContextRuntimeProperties::default();

        if !r.find_tag("RuntimeProperties")? {
            return Ok(None);
        }

        while let Some(tag) = r.next_child()? {
            // We don't expect an XML with namespace here
            if tag.name().prefix().is_some() {
                continue;
            }
            match tag.name().as_ref() {
                b"CrashGUID" => rv.crash_guid = r.value()?,
                b"ProcessId" => rv.process_id = r.value()?,
                b"IsInternalBuild" => rv.is_internal_build = r.value()?,
                b"IsSourceDistribution" => rv.is_source_distribution = r.value()?,
                b"IsAssert" => rv.is_assert = r.value()?,
                b"IsEnsure" => rv.is_ensure = r.value()?,
                b"CrashType" => rv.crash_type = r.value()?,
                b"SecondsSinceStart" => rv.seconds_since_start = r.value()?,
                b"GameName" => rv.game_name = r.value()?,
                b"ExecutableName" => rv.executable_name = r.value()?,
                b"BuildConfiguration" => rv.build_configuration = r.value()?,
                b"PlatformName" => rv.platform_name = r.value()?,
                b"EngineMode" => rv.engine_mode = r.value()?,
                b"EngineVersion" => rv.engine_version = r.value()?,
                b"LanguageLCID" => rv.language_lcid = r.value()?,
                b"AppDefaultLocale" => rv.app_default_locate = r.value()?,
                b"BuildVersion" => rv.build_version = r.value()?,
                b"IsUE4Release" => rv.is_ue4_release = r.value()?,
                b"UserName" => rv.username = r.value()?,
                b"BaseDir" => rv.base_dir = r.value()?,
                b"RootDir" => rv.root_dir = r.value()?,
                b"MachineId" => rv.machine_id = r.value()?,
                b"LoginId" => rv.login_id = r.value()?,
                b"EpicAccountId" => rv.epic_account_id = r.value()?,
                b"CallStack" => rv.legacy_call_stack = r.value()?,
                b"PCallStack" => rv.portable_call_stack = r.value()?,
                b"UserDescription" => rv.user_description = r.value()?,
                b"ErrorMessage" => rv.error_message = r.value()?,
                b"CrashReporterMessage" => rv.crash_reporter_message = r.value()?,
                b"Misc.NumberOfCores" => rv.misc_number_of_cores = r.value()?,
                b"Misc.NumberOfCoresIncludingHyperthreads" => {
                    rv.misc_number_of_cores_inc_hyperthread = r.value()?
                }
                b"Misc.Is64bitOperatingSystem" => rv.misc_is_64bit = r.value()?,
                b"Misc.CPUVendor" => rv.misc_cpu_vendor = r.value()?,
                b"Misc.CPUBrand" => rv.misc_cpu_brand = r.value()?,
                b"Misc.PrimaryGPUBrand" => rv.misc_primary_gpu_brand = r.value()?,
                b"Misc.OSVersionMajor" => rv.misc_os_version_major = r.value()?,
                b"Misc.OSVersionMinor" => rv.misc_os_version_minor = r.value()?,
                b"GameStateName" => rv.game_state_name = r.value()?,
                b"MemoryStats.TotalPhysical" => rv.memory_stats_total_physical = r.value()?,
                b"MemoryStats.TotalVirtual" => rv.memory_stats_total_virtual = r.value()?,
                b"MemoryStats.PageSize" => rv.memory_stats_page_size = r.value()?,
                b"MemoryStats.TotalPhysicalGB" => rv.memory_stats_total_phsysical_gb = r.value()?,
                b"TimeOfCrash" => rv.time_of_crash = r.value()?,
                b"bAllowToBeContacted" => rv.allowed_to_be_contacted = r.value()?,
                b"CrashReportClientVersion" => rv.crash_reporter_client_version = r.value()?,
                b"Modules" => rv.modules = r.value()?,
                _ => {
                    rv.custom.insert(
                        String::from_utf8_lossy(tag.name().as_ref()).to_string(),
                        r.value()?.unwrap_or_default(),
                    );
                }
            }
        }

        Ok(Some(rv))
    }
}

/// Platform specific properties.
///
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L451-L455)
/// [Windows](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/Windows/WindowsPlatformCrashContext.cpp#L39-L44)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Unreal4ContextPlatformProperties {
    /// Whether the crash happened on a Windows device.
    pub is_windows: Option<bool>,
    /// Platform-specific UE4 Core value.
    pub callback_result: Option<i32>,
}

impl Unreal4ContextPlatformProperties {
    fn from_xml(root: &[u8]) -> Result<Option<Self>, quick_xml::Error> {
        let r = quick_xml::Reader::from_reader(root);
        let mut r = XMLReader::new(r);

        let mut rv = Unreal4ContextPlatformProperties::default();

        if !r.find_tag("PlatformProperties")? {
            return Ok(None);
        }

        while let Some(tag) = r.next_child()? {
            if tag.name().as_ref() == b"PlatformIsRunningWindows" {
                if let Some(s) = r.value::<String>()? {
                    match s.parse::<u32>() {
                        Ok(1) => rv.is_windows = Some(true),
                        Ok(0) => rv.is_windows = Some(false),
                        _ => {}
                    }

                    if let Ok(value) = s.parse::<bool>() {
                        rv.is_windows = Some(value);
                    }
                }
            } else if tag.name().as_ref() == b"PlatformCallbackResult" {
                rv.callback_result = r.value::<i32>()?;
            }
        }

        Ok(Some(rv))
    }
}

/// The context data found in the context xml file.
///
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Unreal4Context {
    /// RuntimeProperties context element.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub runtime_properties: Option<Unreal4ContextRuntimeProperties>,

    /// Platform specific properties.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub platform_properties: Option<Unreal4ContextPlatformProperties>,

    /// Engine data.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub engine_data: BTreeMap<String, String>,

    /// Game data.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub game_data: BTreeMap<String, String>,
}

fn load_data_bag(
    data: &[u8],
    tag: &str,
    dest_data: &mut BTreeMap<String, String>,
) -> Result<(), quick_xml::Error> {
    let r = quick_xml::Reader::from_reader(data);
    let mut r = XMLReader::new(r);

    if r.find_tag(tag)? {
        while let Some(tag) = r.next_child()? {
            let name = String::from_utf8_lossy(tag.name().as_ref()).to_string();
            if let Some(value) = r.value::<String>()? {
                dest_data.insert(name, value);
            }
        }
    }

    Ok(())
}

impl Unreal4Context {
    /// Parses the unreal context XML file.
    pub fn parse(data: &[u8]) -> Result<Self, Unreal4Error> {
        let mut engine_data = BTreeMap::default();
        let mut game_data = BTreeMap::default();

        load_data_bag(data, "EngineData", &mut engine_data)?;
        load_data_bag(data, "GameData", &mut game_data)?;

        Ok(Unreal4Context {
            runtime_properties: Unreal4ContextRuntimeProperties::from_xml(&data)?,
            platform_properties: Unreal4ContextPlatformProperties::from_xml(&data)?,
            engine_data,
            game_data,
        })
    }
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

#[allow(dead_code)]
const ROOT_WITH_GAME_AND_ENGINE_DATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FGenericCrashContext>
    <RuntimeProperties>
    </RuntimeProperties>
    <PlatformProperties>
    </PlatformProperties>
    <EngineData>
        <RHI.IsGPUOverclocked>false</RHI.IsGPUOverclocked>
    </EngineData>
    <GameData>
        <sentry>{&quot;release&quot;:"foo.bar.baz@1.0.0"}</sentry>
    </GameData>
</FGenericCrashContext>
"#;

#[test]
fn test_get_runtime_properties_missing_element() {
    assert!(
        Unreal4ContextRuntimeProperties::from_xml(ONLY_ROOT_NODE.as_bytes())
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_get_platform_properties_missing_element() {
    //let root = Element::from_reader().unwrap();
    assert!(
        Unreal4ContextPlatformProperties::from_xml(ONLY_ROOT_NODE.as_bytes())
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_get_runtime_properties_no_children() {
    //let root = Element::from_reader().unwrap();
    let actual = Unreal4ContextRuntimeProperties::from_xml(ONLY_ROOT_AND_CHILD_NODES.as_bytes())
        .unwrap()
        .expect("default struct");
    assert_eq!(Unreal4ContextRuntimeProperties::default(), actual)
}

#[test]
fn test_get_game_and_engine_data() {
    let actual =
        Unreal4Context::parse(ROOT_WITH_GAME_AND_ENGINE_DATA.as_bytes()).expect("default struct");
    assert_eq!(
        actual
            .engine_data
            .get("RHI.IsGPUOverclocked")
            .map(|x| x.as_str()),
        Some("false")
    );
    assert_eq!(
        actual.game_data.get("sentry").map(|x| x.as_str()),
        Some(r#"{"release":"foo.bar.baz@1.0.0"}"#)
    );
}

#[test]
fn test_deeply_nested_xml() {
    let mut data = r#"<FGenericCrashContext>
    <RuntimeProperties>
    </RuntimeProperties>
    <PlatformProperties>
    </PlatformProperties>
    <EngineData>
        <RHI.IsGPUOverclocked>false</RHI.IsGPUOverclocked>
    </EngineData>
    <GameData>
        <sentry>{&quot;release&quot;:"foo.bar.baz@1.0.0"}</sentry>
    </GameData>"#
        .to_owned();

    for _ in 0..30_000 {
        data.push_str("<n>");
    }
    for _ in 0..30_000 {
        data.push_str("</n>");
    }
    data.push_str("</FGenericCrashContext>");

    let _ = Unreal4Context::parse(data.as_bytes()).unwrap();
}

#[test]
fn test_get_platform_properties_no_children() {
    //let root = Element::from_reader().unwrap();
    let actual = Unreal4ContextPlatformProperties::from_xml(ONLY_ROOT_AND_CHILD_NODES.as_bytes())
        .unwrap()
        .expect("default struct");
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
                let xml = concat!("<", $xml_parent, ">", "<", $xml_elm, ">", $expect, "</", $xml_elm, ">","</", $xml_parent, ">");
                let runtime_properties = $func_name(xml.as_bytes()).unwrap().expect(concat!($xml_parent, " exists"));
                similar_asserts::assert_eq!(
                    $expect,
                    runtime_properties.$name.expect("missing property value")
                );
            }

            #[test]
            fn test_none() {
                #[rustfmt::skip]
                let xml = concat!("<", $xml_parent, ">", "<", $xml_elm, ">","</", $xml_elm, ">","</", $xml_parent, ">");
                let runtime_properties = $func_name(xml.as_bytes()).unwrap().expect(concat!($xml_parent, " exists"));
                assert!(runtime_properties.$name.is_none());
            }
        }
    };
}

macro_rules! test_unreal_runtime_properties {
    ($name:ident, $xml_elm:expr, $expect:expr $(,)*) => {
        test_unreal_contect!(
            "RuntimeProperties",
            Unreal4ContextRuntimeProperties::from_xml,
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
            Unreal4ContextPlatformProperties::from_xml,
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
    r"YetAnother!AActor::IsPendingKillPending()
YetAnother!__scrt_common_main_seh() [f:\dd\vctools\crt\vcstartup\src\startup\exe_common.inl:288]
kernel32
ntdll"
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
    misc_primary_gpu_brand,
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
test_unreal_runtime_properties!(
    modules,
    "Modules",
    r"\\Mac\Home\Desktop\WindowsNoEditor\YetAnother\Binaries\Win64\YetAnother.exe
\\Mac\Home\Desktop\WindowsNoEditor\Engine\Binaries\ThirdParty\Vorbis\Win64\VS2015\libvorbis_64.dll"
);

test_unreal_platform_properties!(is_windows, "PlatformIsRunningWindows", true);
test_unreal_platform_properties!(callback_result, "PlatformCallbackResult", 123);
