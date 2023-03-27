mod from_object;

use indexmap::IndexSet;
use std::collections::HashMap;

pub use from_object::ObjectLineMapping;

/// An internal line mapping.
#[derive(Debug)]
struct LineEntry {
    /// The C++ line that is being mapped.
    cpp_line: u32,
    /// The C# line it corresponds to.
    cs_line: u32,
    /// The index into the `cs_files` [`IndexSet`] below for the corresponding C# file.
    cs_file_idx: usize,
}

/// A parsed Il2Cpp/Unity Line mapping JSON.
#[derive(Debug, Default)]
pub struct LineMapping {
    /// The set of C# files.
    cs_files: IndexSet<String>,
    /// A map of C++ filename to a list of Mappings.
    cpp_file_map: HashMap<String, Vec<LineEntry>>,
}

impl LineMapping {
    /// Parses a JSON buffer into a valid [`LineMapping`].
    ///
    /// Returns [`None`] if the JSON was not a valid mapping.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_slice(data).ok()?;
        let mut result = Self::default();

        if let serde_json::Value::Object(object) = json {
            for (cpp_file, file_map) in object {
                // This is a sentinel value for the originating debug file, which
                // `ObjectLineMapping::to_writer` writes to the file to make it unique
                // (and dependent on the originating debug-id).
                if cpp_file == "__debug-id__" {
                    continue;
                }
                let mut lines = Vec::new();
                if let serde_json::Value::Object(file_map) = file_map {
                    for (cs_file, line_map) in file_map {
                        if let serde_json::Value::Object(line_map) = line_map {
                            let cs_file_idx = result.cs_files.insert_full(cs_file).0;
                            for (from, to) in line_map {
                                let cpp_line = from.parse().ok()?;
                                let cs_line = to.as_u64().and_then(|n| n.try_into().ok())?;
                                lines.push(LineEntry {
                                    cpp_line,
                                    cs_line,
                                    cs_file_idx,
                                });
                            }
                        }
                    }
                }
                lines.sort_by_key(|entry| entry.cpp_line);
                result.cpp_file_map.insert(cpp_file, lines);
            }
        }

        Some(result)
    }

    /// Looks up the corresponding C# file/line for a given C++ file/line.
    ///
    /// As these mappings are not exact, this will return an exact match, or a mapping "close-by".
    pub fn lookup(&self, file: &str, line: u32) -> Option<(&str, u32)> {
        let lines = self.cpp_file_map.get(file)?;

        let idx = match lines.binary_search_by_key(&line, |entry| entry.cpp_line) {
            Ok(idx) => idx,
            Err(0) => return None,
            Err(idx) => idx - 1,
        };

        let entry = lines.get(idx)?;

        // We will return mappings at most 50 lines away from the source line they refer to.
        if line.saturating_sub(entry.cpp_line) > 50 {
            None
        } else {
            Some((self.cs_files.get_index(entry.cs_file_idx)?, entry.cs_line))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_to_mapping(cpp_source: &[u8]) -> LineMapping {
        let mapping =
            HashMap::from([("main.cpp", ObjectLineMapping::parse_source_file(cpp_source))]);
        let mapping_json = serde_json::to_string(&mapping).unwrap();

        LineMapping::parse(mapping_json.as_bytes()).unwrap()
    }

    #[test]
    fn test_lookup() {
        let parsed_mapping = source_to_mapping(
            b"Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // comments
            some expression // 5
            stretching
            over
            multiple lines

            // blank lines

            // and stuff
            // 13
            //<source_info:main.cs:29>
            actual source code // 15
        ",
        );
        assert_eq!(parsed_mapping.lookup("main.cpp", 0), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 1), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 2), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 3), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 4), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 5), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 6), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 7), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 8), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 9), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 10), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 11), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 12), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 13), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 14), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 15), Some(("main.cs", 29)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 16), Some(("main.cs", 29)));
    }

    #[test]
    fn test_lookup_real_code() {
        // This is a special format where we've manually added the expected line number info at
        // the end of each line to make the test easier to read (and write, too).
        let cpp_source = r#"
IL_001f:                                                                        |
    {                                                                           |
        il2cpp_codegen_runtime_class_init_inline(SentrySdk_t74D2EF9D77AF1E      |
        SentrySdk_ConfigureScope_m365F1871733F0C48E4B585067AC55DD27E654BC4      |
        //<source_info:/Scripts/AdditionalButtons.cs:20>                        |
        // Debug.Log("User set: ant");                                          |
        il2cpp_codegen_runtime_class_init_inline(Debug_t8394C7EEAECA3689C2      | /Scripts/AdditionalButtons.cs:20
        Debug_Log_m87A9A3C761FF5C43ED8A53B16190A53D08F818BB(_stringLiteral      | /Scripts/AdditionalButtons.cs:20
        //<source_info:/Scripts/AdditionalButtons.cs:21>                        | /Scripts/AdditionalButtons.cs:20
        // }                                                                    | /Scripts/AdditionalButtons.cs:20
        return;                                                                 | /Scripts/AdditionalButtons.cs:21
    }                                                                           | /Scripts/AdditionalButtons.cs:21
}                                                                               | /Scripts/AdditionalButtons.cs:21
// System.Void AdditionalButtons::CaptureMessageWithContext()                   | /Scripts/AdditionalButtons.cs:21
IL2CPP_EXTERN_C IL2CPP_METHOD_ATTR void AdditionalButtons_CaptureMessageWi      | /Scripts/AdditionalButtons.cs:21
{                                                                               | /Scripts/AdditionalButtons.cs:21
    static bool s_Il2CppMethodInitialized;                                      | /Scripts/AdditionalButtons.cs:21
    if (!s_Il2CppMethodInitialized)                                             | /Scripts/AdditionalButtons.cs:21
    {                                                                           | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&Action_1_t      | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&SentrySdk_      | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&U3CU3Ec_U3      | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&U3CU3Ec_U3      | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&U3CU3Ec_t7      | /Scripts/AdditionalButtons.cs:21
        il2cpp_codegen_initialize_runtime_metadata((uintptr_t*)&_stringLit      | /Scripts/AdditionalButtons.cs:21
        s_Il2CppMethodInitialized = true;                                       | /Scripts/AdditionalButtons.cs:21
    }                                                                           | /Scripts/AdditionalButtons.cs:21
    Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* G_B2_0 = NULL;          | /Scripts/AdditionalButtons.cs:21
    Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* G_B1_0 = NULL;          | /Scripts/AdditionalButtons.cs:21
    Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* G_B4_0 = NULL;          | /Scripts/AdditionalButtons.cs:21
    Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* G_B3_0 = NULL;          | /Scripts/AdditionalButtons.cs:21
    {                                                                           | /Scripts/AdditionalButtons.cs:21
        //<source_info:/Scripts/AdditionalButtons.cs:32>                        | /Scripts/AdditionalButtons.cs:21
        // SentrySdk.ConfigureScope(scope =>                                    | /Scripts/AdditionalButtons.cs:32
        //<source_info:/Scripts/AdditionalButtons.cs:33>                        | /Scripts/AdditionalButtons.cs:32
        // {                                                                    | /Scripts/AdditionalButtons.cs:33
        //<source_info:/Scripts/AdditionalButtons.cs:34>                        | /Scripts/AdditionalButtons.cs:33
        //     scope.Contexts["character"] = new PlayerCharacter                | /Scripts/AdditionalButtons.cs:34
        //<source_info:/Scripts/AdditionalButtons.cs:35>                        | /Scripts/AdditionalButtons.cs:34
        //     {                                                                | /Scripts/AdditionalButtons.cs:35
        //<source_info:/Scripts/AdditionalButtons.cs:36>                        | /Scripts/AdditionalButtons.cs:35
        //         Name = "Mighty Fighter",                                     | /Scripts/AdditionalButtons.cs:36
        //<source_info:/Scripts/AdditionalButtons.cs:37>                        | /Scripts/AdditionalButtons.cs:36
        //         Age = 19,                                                    | /Scripts/AdditionalButtons.cs:37
        //<source_info:/Scripts/AdditionalButtons.cs:38>                        | /Scripts/AdditionalButtons.cs:37
        //         AttackType = "melee"                                         | /Scripts/AdditionalButtons.cs:38
        //<source_info:/Scripts/AdditionalButtons.cs:39>                        | /Scripts/AdditionalButtons.cs:38
        //     };                                                               | /Scripts/AdditionalButtons.cs:39
        //<source_info:/Scripts/AdditionalButtons.cs:40>                        | /Scripts/AdditionalButtons.cs:39
        // });                                                                  | /Scripts/AdditionalButtons.cs:39
        il2cpp_codegen_runtime_class_init_inline(U3CU3Ec_t79DD2293FFBDE9C9      | /Scripts/AdditionalButtons.cs:40
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_0 = ((U3CU3E      | /Scripts/AdditionalButtons.cs:40
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_1 = L_0;          | /Scripts/AdditionalButtons.cs:40
        G_B1_0 = L_1;                                                           | /Scripts/AdditionalButtons.cs:40
        if (L_1)                                                                | /Scripts/AdditionalButtons.cs:40
        {                                                                       | /Scripts/AdditionalButtons.cs:40
            G_B2_0 = L_1;                                                       | /Scripts/AdditionalButtons.cs:40
            goto IL_001f;                                                       | /Scripts/AdditionalButtons.cs:40
        }                                                                       | /Scripts/AdditionalButtons.cs:40
    }                                                                           | /Scripts/AdditionalButtons.cs:40
    {                                                                           | /Scripts/AdditionalButtons.cs:40
        il2cpp_codegen_runtime_class_init_inline(U3CU3Ec_t79DD2293FFBDE9C9      | /Scripts/AdditionalButtons.cs:40
        U3CU3Ec_t79DD2293FFBDE9C9C239DB7D77EA41E31CFCA564* L_2 = ((U3CU3Ec      | /Scripts/AdditionalButtons.cs:40
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_3 = (Action_      | /Scripts/AdditionalButtons.cs:40
        NullCheck(L_3);                                                         | /Scripts/AdditionalButtons.cs:40
        Action_1__ctor_mCE58979C800E29B3B4B1673D24BDF60161B4A942(L_3, L_2,      | /Scripts/AdditionalButtons.cs:40
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_4 = L_3;          | /Scripts/AdditionalButtons.cs:40
        ((U3CU3Ec_t79DD2293FFBDE9C9C239DB7D77EA41E31CFCA564_StaticFields*)      | /Scripts/AdditionalButtons.cs:40
        Il2CppCodeGenWriteBarrier((void**)(&((U3CU3Ec_t79DD2293FFBDE9C9C23      | /Scripts/AdditionalButtons.cs:40
        G_B2_0 = L_4;                                                           | /Scripts/AdditionalButtons.cs:40
    }                                                                           | /Scripts/AdditionalButtons.cs:40
                                                                                | /Scripts/AdditionalButtons.cs:40
IL_001f:                                                                        | /Scripts/AdditionalButtons.cs:40
    {                                                                           | /Scripts/AdditionalButtons.cs:40
        il2cpp_codegen_runtime_class_init_inline(SentrySdk_t74D2EF9D77AF1E      | /Scripts/AdditionalButtons.cs:40
        SentrySdk_ConfigureScope_m365F1871733F0C48E4B585067AC55DD27E654BC4      | /Scripts/AdditionalButtons.cs:40
        //<source_info:/Scripts/AdditionalButtons.cs:42>                        | /Scripts/AdditionalButtons.cs:40
        // SentrySdk.CaptureMessage("Capturing with player character conte      | /Scripts/AdditionalButtons.cs:40
        SentryId_t0035EA63F72CBEBE7C342CD0C3A1D80C91A77940 L_5;                 | /Scripts/AdditionalButtons.cs:42
        L_5 = SentrySdk_CaptureMessage_mEB25F3DA889DE8DC87DB25178C01ADE78F      | /Scripts/AdditionalButtons.cs:42
        //<source_info:/Scripts/AdditionalButtons.cs:43>                        | /Scripts/AdditionalButtons.cs:42
        // SentrySdk.ConfigureScope(scope => scope.Contexts = null);            | /Scripts/AdditionalButtons.cs:42
        il2cpp_codegen_runtime_class_init_inline(U3CU3Ec_t79DD2293FFBDE9C9      | /Scripts/AdditionalButtons.cs:43
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_6 = ((U3CU3E      | /Scripts/AdditionalButtons.cs:43
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_7 = L_6;          | /Scripts/AdditionalButtons.cs:43
        G_B3_0 = L_7;                                                           | /Scripts/AdditionalButtons.cs:43
        if (L_7)                                                                | /Scripts/AdditionalButtons.cs:43
        {                                                                       | /Scripts/AdditionalButtons.cs:43
            G_B4_0 = L_7;                                                       | /Scripts/AdditionalButtons.cs:43
            goto IL_004f;                                                       | /Scripts/AdditionalButtons.cs:43
        }                                                                       | /Scripts/AdditionalButtons.cs:43
    }                                                                           | /Scripts/AdditionalButtons.cs:43
    {                                                                           | /Scripts/AdditionalButtons.cs:43
        il2cpp_codegen_runtime_class_init_inline(U3CU3Ec_t79DD2293FFBDE9C9      | /Scripts/AdditionalButtons.cs:43
        U3CU3Ec_t79DD2293FFBDE9C9C239DB7D77EA41E31CFCA564* L_8 = ((U3CU3Ec      | /Scripts/AdditionalButtons.cs:43
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_9 = (Action_      | /Scripts/AdditionalButtons.cs:43
        NullCheck(L_9);                                                         | /Scripts/AdditionalButtons.cs:43
        Action_1__ctor_mCE58979C800E29B3B4B1673D24BDF60161B4A942(L_9, L_8,      | /Scripts/AdditionalButtons.cs:43
        Action_1_t1C1C5545B1F9348689F2EE7364FC9ACC425526EC* L_10 = L_9;         | /Scripts/AdditionalButtons.cs:43
        ((U3CU3Ec_t79DD2293FFBDE9C9C239DB7D77EA41E31CFCA564_StaticFields*)      | /Scripts/AdditionalButtons.cs:43
        Il2CppCodeGenWriteBarrier((void**)(&((U3CU3Ec_t79DD2293FFBDE9C9C23      | /Scripts/AdditionalButtons.cs:43
        G_B4_0 = L_10;                                                          | /Scripts/AdditionalButtons.cs:43
    }                                                                           | /Scripts/AdditionalButtons.cs:43
                                                                                | /Scripts/AdditionalButtons.cs:43
IL_004f:                                                                        | /Scripts/AdditionalButtons.cs:43
    {                                                                           | /Scripts/AdditionalButtons.cs:43
        il2cpp_codegen_runtime_class_init_inline(SentrySdk_t74D2EF9D77AF1E      | /Scripts/AdditionalButtons.cs:43
        SentrySdk_ConfigureScope_m365F1871733F0C48E4B585067AC55DD27E654BC4      | /Scripts/AdditionalButtons.cs:43
        //<source_info:/Scripts/AdditionalButtons.cs:44>                        | /Scripts/AdditionalButtons.cs:43
        // }                                                                    | /Scripts/AdditionalButtons.cs:43
        return;                                                                 | /Scripts/AdditionalButtons.cs:44
    }                                                                           | /Scripts/AdditionalButtons.cs:44
}                                                                               | /Scripts/AdditionalButtons.cs:44
        "#.trim();
        let cpp_lines = cpp_source.split('\n').collect::<Vec<_>>();
        let clean_cpp_lines = cpp_lines
            .iter()
            .map(|line| line.split_once('|').unwrap().0.trim_end())
            .collect::<Vec<_>>();
        let parsed_mapping = source_to_mapping(clean_cpp_lines.join("\n").as_bytes());

        let cs_lines = cpp_lines.iter().map(|line| {
            line.split('|')
                .last()
                .map(|v| v.splitn(2, ':').collect::<Vec<_>>())
                .and_then(|info| {
                    if info.len() == 2 {
                        Some((info[0].trim(), info[1].parse::<u32>().unwrap()))
                    } else {
                        None
                    }
                })
        });

        for (i, expected) in cs_lines.enumerate() {
            let line_nr = i as u32 + 1;
            println!("Checking line {: >3}: {}", line_nr, clean_cpp_lines[i]);
            assert_eq!(parsed_mapping.lookup("main.cpp", line_nr), expected);
        }
    }
}
