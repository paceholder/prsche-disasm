#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>
namespace porsche {
// Original0048a9a0 is exactly JMP [005b2080], KERNEL32.dll!GetTickCount.
std::uint32_t __stdcall network_lifecycle_tick_count_0048a9a0() {
    return ::GetTickCount();
}
}
