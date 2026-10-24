cmake_minimum_required(VERSION 3.20)
project(porsche_network_member_398 LANGUAGES CXX)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../392-startup-integration" entry-392)
target_sources(porsche_game_entry_bridges PRIVATE
    ../396-network-member-constructor/network_member_constructor.cpp)
