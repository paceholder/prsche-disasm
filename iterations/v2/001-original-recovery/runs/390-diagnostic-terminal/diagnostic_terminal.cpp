#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstring>
#include "diagnostic_terminal.hpp"
#include "porsche/formatter_bootstrap.hpp"
#include "porsche/process_exit.hpp"
#include "porsche/files.hpp"
#include "porsche/file_threads.hpp"
#include "porsche/heap.hpp"

namespace porsche {
std::int32_t startup_diagnostic_mode_005e50b4 = 2;
std::int32_t startup_debugger_present_005deb68 = 1;
std::int32_t __cdecl file_current_thread_0055f780(std::uint32_t);

void __cdecl startup_diagnostic_sink_0059fc20(const char* text) {
    const auto callback = missing_file_006afbe4;
    char buffer[256]; // Original byte253 remains opaque for input length>=253.
    if (callback) callback(text, -2);
    formatter_error_copy_n_005a28a0(buffer, text, 253);
    buffer[254] = 0;
    std::strcat(buffer, "\n");
    const auto mode = startup_diagnostic_mode_005e50b4;
    if (mode == 3) {
        ::OutputDebugStringA(buffer);
        startup_debug_break_005af0c0();
        return;
    }
    if (mode == 1) {
        ::OutputDebugStringA(buffer);
        return;
    }
    if (mode != 2) return;
    if (file_current_thread_0055f780(0)) {
        if (::MessageBoxA(nullptr, buffer, "Error: terminate execution?", 0x51034) == 7) return;
        const auto module = ::LoadLibraryA("kernel32.dll");
        if (module) {
            const auto address = ::GetProcAddress(module, "IsDebuggerPresent");
            if (address) {
                const auto query = reinterpret_cast<BOOL (WINAPI*)()>(address);
                startup_debugger_present_005deb68 = query();
            }
        }
        if (startup_debugger_present_005deb68) {
            ::OutputDebugStringA(buffer);
            startup_debug_break_005af0c0();
        }
        process_exit_005a246e(1);
    }
    if (thread_start_lock_006a57d8) heap_enter_005322b0(thread_start_lock_006a57d8);
    ::MessageBoxA(nullptr, buffer, "Error: execution may continue.", 0x51030);
    if (thread_start_lock_006a57d8) heap_leave_005322c0(thread_start_lock_006a57d8);
}
}
