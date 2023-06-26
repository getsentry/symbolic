import os
import re
import sys
import atexit
import shutil
import zipfile
import tempfile
import subprocess
from setuptools import setup, find_packages
from setuptools.command.sdist import sdist


_version_re = re.compile(r'(?m)^version\s*=\s*"(.*?)"\s*$')


DEBUG_BUILD = os.environ.get("SYMBOLIC_DEBUG") == "1"

with open("README") as f:
    readme = f.read()


if os.path.isfile("../symbolic-cabi/Cargo.toml"):
    with open("../symbolic-cabi/Cargo.toml") as f:
        match = _version_re.search(f.read())
        assert match is not None
        version = match[1]
else:
    with open("version.txt") as f:
        version = f.readline().strip()


def vendor_rust_deps():
    subprocess.Popen(["scripts/git-archive-all", "py/rustsrc.zip"], cwd="..").wait()


def write_version():
    with open("version.txt", "w") as f:
        f.write("%s\n" % version)


class CustomSDist(sdist):
    def run(self):
        vendor_rust_deps()
        write_version()
        sdist.run(self)


def build_native(spec):
    cmd = ["cargo", "build", "-p", "symbolic-cabi"]
    if not DEBUG_BUILD:
        cmd.append("--release")
        target = "release"
    else:
        target = "debug"

    # Step 0: find rust sources
    if not os.path.isfile("../symbolic-cabi/Cargo.toml"):
        scratchpad = tempfile.mkdtemp()

        @atexit.register
        def delete_scratchpad():
            try:
                shutil.rmtree(scratchpad)
            except OSError:
                pass

        zf = zipfile.ZipFile("rustsrc.zip")
        zf.extractall(scratchpad)
        rust_path = scratchpad + "/rustsrc"
    else:
        rust_path = ".."
        scratchpad = None

    # Step 1: build the rust library
    print("running `{}` ({} target)".format(" ".join(cmd), target))
    build = spec.add_external_build(cmd=cmd, path=rust_path)

    def find_dylib():
        cargo_target = os.environ.get("CARGO_BUILD_TARGET")
        if cargo_target:
            in_path = f"target/{cargo_target}/{target}"
        else:
            in_path = "target/%s" % target
        return build.find_dylib("symbolic_cabi", in_path=in_path)

    rtld_flags = ["NOW"]
    if sys.platform == "darwin":
        rtld_flags.append("NODELETE")
    spec.add_cffi_module(
        module_path="symbolic._lowlevel",
        dylib=find_dylib,
        header_filename=lambda: build.find_header(
            "symbolic.h", in_path="symbolic-cabi/include"
        ),
        rtld_flags=rtld_flags,
    )


setup(
    name="symbolic",
    version=version,
    packages=find_packages(),
    author="Sentry",
    license="MIT",
    author_email="hello@sentry.io",
    description="A python library for dealing with symbol files and more.",
    long_description=readme,
    include_package_data=True,
    package_data={"symbolic": ["py.typed", "_lowlevel.pyi"]},
    zip_safe=False,
    platforms="any",
    install_requires=["milksnake>=0.1.2"],
    setup_requires=["milksnake>=0.1.2"],
    python_requires=">=3.8",
    milksnake_tasks=[build_native],
    cmdclass={"sdist": CustomSDist},
)
