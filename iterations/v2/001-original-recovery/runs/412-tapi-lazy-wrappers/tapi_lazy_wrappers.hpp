#pragma once
#include <cstdint>
namespace porsche {
extern std::uint32_t tapi_drop_cache_006ad0b8;
extern std::uint32_t tapi_devcaps_cache_006ad15c;
extern std::uint32_t tapi_getid_cache_006ad188;
extern std::uint32_t tapi_open_cache_006ad20c;
extern std::uint32_t tapi_status_cache_006ad2bc;
std::int32_t __stdcall modem_line_drop_00584b70(std::uint32_t,const void*,std::uint32_t);
std::int32_t __stdcall modem_line_get_capabilities_00584bb7(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,void*);
std::int32_t __stdcall modem_line_get_id_00584c4c(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,void*,const char*);
std::int32_t __stdcall modem_tapi_line_open_00584dcd(std::uint32_t,std::uint32_t,std::uint32_t*,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,const void*);
std::int32_t __stdcall modem_line_set_status_00584e72(std::uint32_t,std::uint32_t,std::uint32_t);
// Original 75B resolver remains an unresolved real child (stdcall, RET8).
std::int32_t __stdcall tapi_resolve_00584a60(const char*,std::uint32_t*);
}
