cmake_minimum_required(VERSION 3.20)
project(porsche_modem_completion_413 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../409-tapi-modem-integration" entry-409)
target_sources(porsche_game_entry_bridges PRIVATE
    ../411-modem-callback/modem_callback.cpp
    ../412-tapi-lazy-wrappers/tapi_lazy_wrappers.cpp)
target_include_directories(porsche_game_entry_bridges PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../411-modem-callback"
    "${CMAKE_CURRENT_SOURCE_DIR}/../412-tapi-lazy-wrappers")
