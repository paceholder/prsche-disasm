#pragma once
#include "modem_enumeration.hpp"
namespace porsche {
// Original pointer token is only tested here; no original VA is dereferenced.
extern std::uint32_t modem_configuration_006a57a8;
void __cdecl modem_capabilities_clear_0055de50();
std::int32_t __stdcall modem_line_get_capabilities_00584bb7(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,void*);
std::int32_t __stdcall modem_tapi_line_open_00584dcd(std::uint32_t,std::uint32_t,std::uint32_t*,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,const void*);
std::int32_t __stdcall modem_line_set_status_00584e72(std::uint32_t,std::uint32_t,std::uint32_t);
std::uint32_t __cdecl modem_line_configure_0055e340(std::uint32_t);
}
