cmake_minimum_required(VERSION 3.22.1)

project(lan_audio_opus_jni LANGUAGES C CXX)

include(FetchContent)

set(OPUS_BUILD_PROGRAMS OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(OPUS_BUILD_SHARED_LIBRARY OFF CACHE BOOL "" FORCE)
set(OPUS_INSTALL_PKG_CONFIG_MODULE OFF CACHE BOOL "" FORCE)
set(OPUS_INSTALL_CMAKE_CONFIG_MODULE OFF CACHE BOOL "" FORCE)
set(OPUS_CUSTOM_MODES OFF CACHE BOOL "" FORCE)
set(OPUS_FIXED_POINT OFF CACHE BOOL "" FORCE)

FetchContent_Declare(
    opus
    GIT_REPOSITORY https://github.com/xiph/opus.git
    GIT_TAG v1.5.2
    GIT_SHALLOW TRUE
)
set(OBOE_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(OBOE_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(OBOE_BUILD_TOOLS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(
    oboe
    GIT_REPOSITORY https://github.com/google/oboe.git
    GIT_TAG 1.8.1
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(opus)
FetchContent_MakeAvailable(oboe)

add_library(
    lan_audio_opus_jni
    SHARED
    opus_jni.cpp
    oboe_sink.cpp
)

target_compile_features(lan_audio_opus_jni PRIVATE cxx_std_17)

target_link_libraries(lan_audio_opus_jni PRIVATE opus oboe log OpenSLES)
