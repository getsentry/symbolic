dotnet build src -c Release --no-incremental
cp src/bin/Release/*/*.dll .
cp src/bin/Release/*/*.pdb .
