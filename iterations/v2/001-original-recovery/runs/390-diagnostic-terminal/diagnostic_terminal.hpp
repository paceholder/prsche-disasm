#pragma once
#include <cstdint>
namespace porsche {
extern std::int32_t startup_diagnostic_mode_005e50b4;
extern std::int32_t startup_debugger_present_005deb68;
void __cdecl startup_diagnostic_sink_0059fc20(const char* text);
// Original26B helper remains unrecovered; never replace with a guessed DebugBreak.
void __cdecl startup_debug_break_005af0c0();
}
