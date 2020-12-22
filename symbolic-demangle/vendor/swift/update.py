#!/usr/bin/env python3

import os
import shutil
import subprocess
import sys


SWIFT_PATH = "swift"
DEMANGLING_PATH = "lib/Demangling"
WORKSPACE_INCLUDES = [
    'llvm-project/llvm/include',
    'build/Ninja-DebugAssert/llvm-macosx-x86_64/include',
    'build/Ninja-DebugAssert/swift-macosx-x86_64/include',
    'swift/include',
]
FATAL_ERROR = b" fatal error: "


def print_usage():
    print("Updates swift demangler sources. Requires a valid swift checkout.")
    print()
    print("Usage: %s <path-to-swift-workspace>" % (sys.argv[0],))


def get_headers(source_file, demangler_target, workspace_dir):
    if not source_file.endswith(".cpp"):
        return []

    includes = ["-I%s" % os.path.join(workspace_dir, include) for include in WORKSPACE_INCLUDES]
    args = [
        "clang",
        "-E",  # Only run the preprocessor
        "-H",  # Show header includes and nesting depth
        *includes,
        os.path.join(demangler_target, source_file)
    ]

    result = subprocess.run(args, capture_output=True)
    lines = result.stderr.splitlines()
    if result.returncode != 0:
        print("ERROR while resolving headers for %s" % source_file)
        for error in lines:
            if FATAL_ERROR in error:
                print(" ", error.split(FATAL_ERROR, 1)[1].decode("utf8"))
        return []

    headers = []
    for line in lines:
        # The output of `-H` starts with a series of dots and a space
        if not line.startswith(b"."):
            continue

        header = line.split(b" ", 1)[1].decode("utf8")
        if header.startswith(workspace_dir):
            headers.append(header)

    return headers


def copy_header(header, vendor_dir, workspace_dir):
    header = os.path.realpath(header)
    relative_path = os.path.relpath(header, workspace_dir)

    include_index = relative_path.find("/include/")
    if include_index > 0:
        relative_path = relative_path[include_index + 1:]
    elif relative_path.startswith("swift/"):
        relative_path = relative_path[6:]
    else:
        print("  WARN: Skipping header outside of include/ or swift/")
        return

    target_path = os.path.join(vendor_dir, relative_path)
    target_dir = os.path.dirname(target_path)

    if not os.path.exists(target_dir):
        os.makedirs(target_dir)

    if os.path.exists(target_path):
        os.remove(target_path)

    shutil.copy2(header, target_path)


def main():
    if len(sys.argv) != 2 or sys.argv[1] in ("-h", "help"):
        print_usage()
        exit()

    workspace_dir = os.path.realpath(sys.argv[1])
    swift_dir = os.path.join(workspace_dir, SWIFT_PATH)
    if not os.path.isdir(swift_dir):
        print("ERROR: No swift workspace found at %s" % (workspace_dir,))
        print("Refer to README.md for instructions.")
        exit()

    vendor_dir = os.path.dirname(os.path.realpath(__file__))

    print("Updating swift sources.")
    print("  Source: %s" % (workspace_dir,))
    print("  Target: %s" % (vendor_dir))
    print()

    print("> Cleaning up previous import")
    for entry in os.listdir(vendor_dir):
        if os.path.isdir(entry) and not workspace_dir.startswith(os.path.join(vendor_dir, entry)):
            shutil.rmtree(entry)

    print("> Replacing sources in %s" % (DEMANGLING_PATH))
    demangler_source = os.path.join(swift_dir, DEMANGLING_PATH)
    demangler_target = os.path.join(vendor_dir, DEMANGLING_PATH)
    shutil.copytree(demangler_source, demangler_target)

    print("> Resolving required headers")
    required_headers = set()
    for source_file in os.listdir(demangler_target):
        required_headers.update(get_headers(source_file, demangler_target, workspace_dir))

    print("> Copying %s headers" % (len(required_headers),))
    for header in required_headers:
        copy_header(header, vendor_dir, workspace_dir)

    print()
    print("Done. Please run `git status` to check for added or removed sources.")


if __name__ == "__main__":
    main()
