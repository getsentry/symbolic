//! Unreal Engine 4 crash context information
#![warn(missing_docs)]

extern crate failure;

use Unreal4Crash;
use Unreal4Error;
use Unreal4FileType;

use elementtree::{Element, QName};

/// The context data found in the context xml file.
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp)
pub struct Unreal4Context {
    pub runtime_properties: Option<Unreal4ContextRuntimeProperties>,
    pub platform_properties: Option<Unreal4ContextPlatformProperties>,
}

/// RuntimeProperties context element
/// [Source](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L274)
#[derive(Default, Clone, Debug)]
pub struct Unreal4ContextRuntimeProperties {
    /// CrashGUID
    pub crash_guid: Option<String>,
    /// ProcessId
    pub process_id: Option<u32>,
}

impl Unreal4ContextRuntimeProperties {
    fn empty() -> Unreal4ContextRuntimeProperties {
        Unreal4ContextRuntimeProperties {
            crash_guid: None,
            process_id: None,
        }
    }
}

/// Platform specific properties.
/// [Source[(https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/GenericPlatform/GenericPlatformCrashContext.cpp#L451-L455)
/// [Windows](https://github.com/EpicGames/UnrealEngine/blob/b70f31f6645d764bcb55829228918a6e3b571e0b/Engine/Source/Runtime/Core/Private/Windows/WindowsPlatformCrashContext.cpp#L39-L44)
pub struct Unreal4ContextPlatformProperties {
    /// Whether the crash happened on a Windows device.
    pub is_windows: Option<bool>,
    /// Platform-specific UE4 Core value.
    pub callback_result: Option<i32>,
}

impl Unreal4ContextPlatformProperties {
    fn empty() -> Unreal4ContextPlatformProperties {
        Unreal4ContextPlatformProperties {
            is_windows: None,
            callback_result: None,
        }
    }
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

    let mut rv = Unreal4ContextRuntimeProperties::empty();

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

    let mut rv = Unreal4ContextPlatformProperties::empty();

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
fn test_get_runtime_properties() {
    let root = Element::from_reader(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<FGenericCrashContext>
	<RuntimeProperties>
		<CrashGUID>UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000</CrashGUID>
	</RuntimeProperties>
</FGenericCrashContext>
"#.as_bytes(),
    ).unwrap();

    let runtime_properties = get_runtime_properties(&root).expect("RuntimeProperties exists");
    assert_eq!(
        "UE4CC-Windows-379993BB42BD8FBED67986857D8844B5_0000",
        runtime_properties.crash_guid.expect("crash guid")
    );
}
