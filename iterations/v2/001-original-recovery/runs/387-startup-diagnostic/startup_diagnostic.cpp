#include "startup_diagnostic.hpp"
#include "../353-diagnostic-mono/diagnostic_mono.hpp"
#include "porsche/formatter_entry.hpp"
#include "porsche/shared_runtime_globals.hpp"
#include <cstdarg>
#include <cstdint>
#include <cstring>
namespace porsche {
const char startup_diagnostic_context_005e50b8[20]=" [FILE=%s, LINE=%d]";
void __cdecl startup_diagnostic_callback_0059fbc0(const char* format,...){
    char buffer[1024];
    std::va_list arguments;va_start(arguments,format);
    static_assert(sizeof(arguments)==sizeof(void*),"Original x86 raw varargs");
    (void)application_diagnostic_mono_vsprintf_005a299e(buffer,format,
        reinterpret_cast<const std::uint32_t*>(arguments));
    va_end(arguments);
    // Original reads both shared cells AFTER the first formatter has returned,
    // then scans its output and appends at its terminatingNUL.
    const auto line=diagnostic_line_005deb78;
    const char* file=diagnostic_file_005deb74();
    const auto length=std::strlen(buffer);
    (void)formatter_entry_005a0fbf(buffer+length,startup_diagnostic_context_005e50b8,file,line);
    startup_diagnostic_sink_0059fc20(buffer);
}
}
