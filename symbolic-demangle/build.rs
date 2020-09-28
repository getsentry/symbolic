fn main() {
    #[cfg(feature = "swift")]
    {
        cc::Build::new()
            .cpp(true)
            .files(&[
                "src/swiftdemangle.cpp",
                "vendor/swift/lib/Demangling/Demangler.cpp",
                "vendor/swift/lib/Demangling/Context.cpp",
                "vendor/swift/lib/Demangling/ManglingUtils.cpp",
                "vendor/swift/lib/Demangling/NodeDumper.cpp",
                "vendor/swift/lib/Demangling/NodePrinter.cpp",
                "vendor/swift/lib/Demangling/OldDemangler.cpp",
                // "vendor/swift/lib/Demangling/OldRemangler.cpp",
                "vendor/swift/lib/Demangling/Punycode.cpp",
                "vendor/swift/lib/Demangling/Remangler.cpp",
            ])
            .flag_if_supported("-std=c++14")
            .flag("-DLLVM_DISABLE_ABI_BREAKING_CHECKS_ENFORCING=1")
            .warnings(false)
            .include("vendor/swift/include")
            .compile("swiftdemangle");
    }
}
