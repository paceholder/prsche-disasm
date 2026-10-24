cmake_minimum_required(VERSION 3.20)
project(porsche_platform_file_406 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../402-debug-modem-integration" entry-402)
target_sources(porsche_game_entry_bridges PRIVATE
    ../404-platform-facades/platform_facades.cpp
    ../405-file-backends/file_backends.cpp)
