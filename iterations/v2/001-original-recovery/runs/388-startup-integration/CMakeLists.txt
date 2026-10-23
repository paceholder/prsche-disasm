cmake_minimum_required(VERSION 3.20)
project(porsche_startup_388 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../384-startup-render-integration" entry-384)
target_sources(porsche_game_entry_bridges PRIVATE
    ../386-heap-startup/heap_startup.cpp
    ../387-startup-diagnostic/startup_diagnostic.cpp)
