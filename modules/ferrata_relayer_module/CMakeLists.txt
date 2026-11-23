# ferrata_relayer_module: the module is the Rust crate under rust-lib/. The builder compiles it to a
# static library and links it into the plugin with the generated Qt glue (the builder's Rust template).

cmake_minimum_required(VERSION 3.14)
project(FerrataRelayerModulePlugin LANGUAGES CXX)

if(DEFINED ENV{LOGOS_MODULE_BUILDER_ROOT})
    include($ENV{LOGOS_MODULE_BUILDER_ROOT}/cmake/LogosModule.cmake)
elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/cmake/LogosModule.cmake")
    include(cmake/LogosModule.cmake)
else()
    message(FATAL_ERROR "LogosModule.cmake not found")
endif()

file(READ "${CMAKE_CURRENT_SOURCE_DIR}/metadata.json" METADATA_JSON)
string(JSON MODULE_NAME GET ${METADATA_JSON} name)

logos_module(NAME ${MODULE_NAME})

# macOS: risc0-zkp (the receipt verifier, and the Groth16 step's input.json) compiles its Metal prover into the
# same object files as its verifier for aarch64-apple-darwin, so the plugin's link needs Metal and the Objective-C
# runtime (MTLCreateSystemDefaultDevice, MTLCopyAllDevices, objc_msgSend, ...); LogosModule.cmake adds only
# CoreFoundation and Security for a Rust module. The prover is never called: the relayer proves through r0vm,
# and flake.nix leaves its kernel library empty (RISC0_SKIP_BUILD_KERNELS).
if(APPLE)
    target_link_libraries(${MODULE_NAME}_module_plugin PRIVATE "-framework Metal" objc)
endif()
