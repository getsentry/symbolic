#!/bin/bash

# This script was used to generate the fixtures in the "gen" directory, used to test the
# `ElfObject::debug_link` method.

# Pre-requisites:
#
# - gcc
# - eu-elfcompress (elfutils)
# - objcopy (GNU Binary Utilities)
# - crc32 (perl-archive-zip)

OUTPUT=gen

# 0. Clean and remake output directory, switch to it
rm -rf $OUTPUT
mkdir -p $OUTPUT
cd $OUTPUT

# 1. compile our C example. To keep size low, let's compile the simplest program we can write.
gcc -x c -Os -o elf_without_debuglink - << EOF
int main() {
    return 0;
}
EOF

# 2. generate some fake debug file. objcopy doesn't require the file to be a proper debug file,
# only that it exists (to compute its CRC and embed it in the section).
# Let's use a simple text file.
echo "Fake debug info" > debug_info.txt

# 3. compute the expected CRC for debug_info.txt. 
# This will be used in tests to check we find the correct CRC.
crc32 debug_info.txt > debug_info.txt.crc

# 4. Add the debug info to a copy of our binary
objcopy --add-gnu-debuglink=debug_info.txt elf_{without,with}_debuglink

# 5. To test for the various possible paddings, also add debug info 
# with different-sized filenames
cp debug_info{,1}.txt && objcopy --add-gnu-debuglink=debug_info1.txt elf_{without,with1}_debuglink
cp debug_info{,12}.txt && objcopy --add-gnu-debuglink=debug_info12.txt elf_{without,with12}_debuglink
cp debug_info{,123}.txt && objcopy --add-gnu-debuglink=debug_info123.txt elf_{without,with123}_debuglink

# 6. To test the "Owned" case, let's make a copy of the ELF with a compressed section
eu-elfcompress -v --force --name ".gnu_debuglink" -t zlib -o elf_with{_compressed,}_debuglink

# 7. Remove debug info files that aren't actually needed by the tests
rm *.txt
