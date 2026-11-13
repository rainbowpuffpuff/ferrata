# ferrata_updater_module: the module is the Rust crate under rust-lib/. The builder compiles it to a
# static library and links it into the plugin with the generated Qt glue (the builder's Rust template).

cmake_minimum_required(VERSION 3.14)
project(FerrataUpdaterModulePlugin LANGUAGES CXX)

if(DEFINED ENV{LOGOS_MODULE_BUILDER_ROOT})
    include($ENV{LOGOS_MODULE_BUILDER_ROOT}/cmake/LogosModule.cmake)
elseif(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/cmake/LogosModule.cmake")
    include(cmake/LogosModule.cmake)
else()
    message(FATAL_ERROR "LogosModule.cmake not found")
endif()

file(READ "${CMAKE_CURRENT_SOURCE_DIR}/metadata.json" METADATA_JSON)
string(JSON MODULE_NAME GET ${METADATA_JSON} name)

# The sequencer backend links LEZ's wallet, whose keycard support calls PC/SC (pcsc-sys: libpcsclite, a shared
# library the Rust static library leaves undefined). logos_module's LINK_LIBRARIES come after the Rust archive on the
# link line; metadata.json's nix.packages.runtime puts pcsclite in the build. (The builder at the pinned revision reads
# nix.cmake.extra_link_libraries but does not pass it on.)
logos_module(NAME ${MODULE_NAME} LINK_LIBRARIES pcsclite)
