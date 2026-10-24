#pragma once
#include <cstdint>
#include "device_table.hpp"
#include "render_descriptor.hpp"
namespace porsche {
extern std::uint32_t modem_selected_device_005df478;
extern std::uint32_t modem_line_006a4668;
extern std::uint32_t modem_application_006a5670;
extern std::uint32_t modem_line_count_006a5774;
extern std::uint32_t modem_open_mode_006a579c;
extern std::uint32_t modem_api_version_006a57a4;
extern std::uint32_t modem_capabilities_006a57b4;
extern const char* modem_message_006ad43c;
extern const char modem_shutdown_error_005bc98c[21];
using ModemCallback=void(__stdcall*)(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t);
// Original callback0055df80 is a real unresolved provider. RET24 and installed
// x86 SDK LINECALLBACK establish ABI; no raw original VA is used as a host pointer.
void __stdcall modem_callback_0055df80(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t);
std::int32_t __stdcall modem_line_initialize_00584ce0(std::uint32_t*,std::uint32_t,ModemCallback,const char*,std::uint32_t*);
std::int32_t __stdcall modem_line_negotiate_00584d7c(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t*,void*);
std::int32_t __stdcall modem_line_status_00584c9d(std::uint32_t,void*);
std::int32_t __stdcall modem_line_shutdown_00584eb9(std::uint32_t);
std::int32_t __stdcall modem_line_close_00584af2(std::uint32_t);
std::int32_t __cdecl modem_device_capabilities_0055e6e0(std::uint32_t);
std::int32_t __cdecl modem_line_open_0055e270();
}
