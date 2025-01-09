fn main() {
    #[cfg(feature = "swift")]
    {
        cc::Build::new()
            .cpp(true)
            .files(&[
                "src/swiftdemangle.cpp",
                "vendor/swift/lib/Demangling/Context.cpp",
                "vendor/swift/lib/Demangling/CrashReporter.cpp",
                "vendor/swift/lib/Demangling/Demangler.cpp",
                "vendor/swift/lib/Demangling/Errors.cpp",
                "vendor/swift/lib/Demangling/ManglingUtils.cpp",
                "vendor/swift/lib/Demangling/NodeDumper.cpp",
                "vendor/swift/lib/Demangling/NodePrinter.cpp",
                // "vendor/swift/lib/Demangling/OldDemangler.cpp",
                // "vendor/swift/lib/Demangling/OldRemangler.cpp",
                "vendor/swift/lib/Demangling/Punycode.cpp",
                "vendor/swift/lib/Demangling/Remangler.cpp",
            ])
            .flag_if_supported("-std=c++17")
            .flag_if_supported("-mmacosx-version-min=11.0.0")
            .flag("-DLLVM_DISABLE_ABI_BREAKING_CHECKS_ENFORCING=1")
            .flag("-DSWIFT_STDLIB_HAS_TYPE_PRINTING=1")
            .warnings(false)
            .include("vendor/swift/include")
            .compile("swiftdemangleabc");
    }
}
