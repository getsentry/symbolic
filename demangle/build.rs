extern crate gcc;

fn main() {
    gcc::Build::new()
        .cpp(true)
        .files(&[
            "src/swiftdemangle.cpp",
            "vendor/swift/lib/Demangling/Context.cpp",
            "vendor/swift/lib/Demangling/ManglingUtils.cpp",
            "vendor/swift/lib/Demangling/NodePrinter.cpp",
            "vendor/swift/lib/Demangling/OldRemangler.cpp",
            "vendor/swift/lib/Demangling/Remangler.cpp",
            "vendor/swift/lib/Demangling/Demangler.cpp",
            "vendor/swift/lib/Demangling/NodeDumper.cpp",
            "vendor/swift/lib/Demangling/OldDemangler.cpp",
            "vendor/swift/lib/Demangling/Punycode.cpp",
        ])
        .flag("-std=c++11")
        .warnings(false)
        .include("vendor/swift/include")
        .compile("swiftdemangle");
}
