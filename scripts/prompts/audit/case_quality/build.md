
================================================================
REALISM_RULES (build type — be strict about compiler/build tool output)
================================================================

R1. Compiler / tool coherence.
    - The case must be from one build tool (xcodebuild / gradle / gcc / maven
      / cargo / dotnet / webpack / etc).
    - Mixing xcode and gradle output in one case -> fabricated.
    - Different compiler error formats (clang vs gcc) in one case -> fabricated.

R2. File path plausibility.
    - Real paths: `src/main.cpp`, `pkg/foo/bar.go`, `lib/util.rs`.
    - Paths in `/Users/x/...` or `C:\Users\...` are sometimes real (developer
      machines) but should match the tool.
    - Paths in `/tmp/` for build artifacts -> suspicious (builds rarely use /tmp).

R3. Error format fidelity.
    - gcc/clang: `file:line:col: error: ...`
    - javac: `symbol: ...\nlocation: ...`
    - rustc: `error[E0xxx]: ... --> file:line:col`
    - xcodebuild: `<unknown>:0: error: ...`
    - Random text in `error:` position -> suspicious.

R4. Progress / status markers.
    - gradle: `[1/10] compile ...` `[BUILDER] ...`
    - webpack: `Hash: abc... Version: webpack 5.x`
    - cargo: `Compiling foo v0.1.0`
    - Missing version stamp for the build tool -> suspicious.

R5. Empty-output authenticity.
    - Build success can be terse (`[100%] Built target foo`).
    - A "build success" with 0 lines and no tool header -> fabricated.

R6. Linking vs compiling separation.
    - Real builds show `Compiling X` then `Linking Y` (or `Assembling`).
    - A "build" that ONLY has `Compiling` lines but ends with errors typical
      of linking -> fabricated.

R7. Dependency / toolchain plausibility.
    - `gcc` on macOS without `Xcode` or `brew` hint -> suspicious.
    - `dotnet build` on Linux without `mono` hint -> suspicious.
    - `xcodebuild` on Linux -> fabricated.
