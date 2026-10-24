#pragma once
#include <cstdint>
namespace porsche {
extern std::uint32_t device_table_count_006a57a0;
extern std::uint32_t device_table_pointer_006a57b8;
// Frozen391 facade name/ABI; original is a signed-index, 12-byte table accessor.
std::uint32_t __cdecl network_environment_0055eb70(std::uint32_t index);
void __cdecl device_table_clear_0055ddf0();
// Original enumeration remains an unresolved real provider.
std::uint32_t __cdecl device_table_enumerate_0055e4d0();
}
