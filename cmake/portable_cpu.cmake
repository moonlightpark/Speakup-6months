# Release builds for Windows: whisper.cpp (ggml) must not be tuned to the build machine's CPU.
# With GGML_NATIVE on, MSVC builds probe the CI runner and compile with /arch:AVX512, and the
# app then dies without a word on PCs without AVX-512 as soon as the speech model loads.
# Off, ggml targets AVX2 (every x86-64 PC since about 2013-2015).
# Passed in by CI as CMAKE_PROJECT_INCLUDE_BEFORE (whisper-rs-sys forwards CMAKE_* variables).
set(GGML_NATIVE OFF CACHE BOOL "portable release build" FORCE)
set(GGML_AVX512 OFF CACHE BOOL "portable release build" FORCE)
