#pragma once
#include "porsche/startup_services.hpp"
#include <cstdint>
namespace porsche {
extern const char startup_network_labels_005d079c[44];
extern std::uint32_t startup_network_timeout_005df6a8;
void* __fastcall network_construct_0048fc80(void* receiver,void* unused_edx);
// Original children whose algorithms remain outside this constructor proof.
std::uint32_t __cdecl network_option_integer_00560a90(const char* name);
std::uint32_t __cdecl network_option_present_005607d0(const char* name);
void* __fastcall network_member_construct_004902c0(void* receiver,void* unused_edx,const void* opaque);
std::uint32_t __cdecl network_environment_0055eb70(std::uint32_t zero);
void* __fastcall network_word_construct_0052df00(void* receiver,void* unused_edx);
void __cdecl network_callbacks_start_00560340();
void __cdecl network_missing_file_0048d9b0(const char*,std::int32_t);
}
