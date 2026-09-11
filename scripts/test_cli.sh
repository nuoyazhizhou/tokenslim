#!/bin/bash
echo "Testing CLI compression..."
echo "src/main.cpp:42:15: error: invalid conversion from 'int' to 'char*'" > test_input.log
echo "make[2]: Entering directory '/home/user/project/build'" >> test_input.log
echo "Exception in thread \"main\" java.lang.NullPointerException" >> test_input.log

cargo run --release -- --compress --input test_input.log --output compressed.tsl --verbose

echo "Testing CLI decompression..."
cargo run --release -- --decompress --input compressed.tsl --output decompressed.log --verbose

echo "Diffing..."
diff test_input.log decompressed.log
if [ $? -eq 0 ]; then
    echo "Success! Perfect match."
else
    echo "Failed! Files differ."
fi

rm test_input.log compressed.tsl decompressed.log
