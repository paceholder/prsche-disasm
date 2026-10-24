#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <intrin.h>
#include "debug_runtime.hpp"
#include "porsche/window_event_translation.hpp"

namespace porsche {
std::int32_t __cdecl startup_debugger_query_005af090() {
    const auto module = ::LoadLibraryA("kernel32.dll");
    if (module) {
        const auto address = ::GetProcAddress(module,"IsDebuggerPresent");
        if (address) {
            const auto query = reinterpret_cast<BOOL (WINAPI*)()>(address);
            return query();
        }
    }
    return 0;
}

void __cdecl startup_debug_break_005af0c0() {
    if (!window_event_flag_005de020) return;
    if (startup_debugger_query_005af090()) {
        __debugbreak(); // Original005af0d2: real INT3, distinct from imported API.
        return;
    }
    ::DebugBreak();
}
}
