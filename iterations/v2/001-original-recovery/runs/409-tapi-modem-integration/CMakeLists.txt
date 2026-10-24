cmake_minimum_required(VERSION 3.20)
project(porsche_tapi_modem_409 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../407-modem-integration" entry-407)
target_sources(porsche_game_entry_bridges PRIVATE
    ../408-tapi-wrappers/tapi_wrappers.cpp
    ../410-modem-caps-open/modem_caps_open.cpp)
target_include_directories(porsche_game_entry_bridges PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../408-tapi-wrappers"
    "${CMAKE_CURRENT_SOURCE_DIR}/../403-modem-enumeration")
