cmake_minimum_required(VERSION 3.20)
project(porsche_startup_392 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../388-startup-integration" entry-388)
target_sources(porsche_game_entry_bridges PRIVATE
    ../390-diagnostic-terminal/diagnostic_terminal.cpp
    ../391-network-constructor/network_constructor.cpp
    ../393-network-constructor-leaves/network_constructor_leaves.cpp
    ../394-network-clock-binding/network_clock.cpp)
