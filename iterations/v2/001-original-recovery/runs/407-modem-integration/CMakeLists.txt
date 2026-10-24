cmake_minimum_required(VERSION 3.20)
project(porsche_modem_407 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../406-platform-file-integration" entry-406)
target_sources(porsche_game_entry_bridges PRIVATE ../403-modem-enumeration/modem_enumeration.cpp)
target_include_directories(porsche_game_entry_bridges PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../401-device-table"
    "${CMAKE_CURRENT_SOURCE_DIR}/../374-render-descriptor")
