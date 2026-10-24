cmake_minimum_required(VERSION 3.20)
project(porsche_debug_modem_402 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../398-network-member-integration" entry-398)
target_sources(porsche_game_entry_bridges PRIVATE
    ../400-debug-runtime/debug_runtime.cpp
    ../401-device-table/device_table.cpp)
