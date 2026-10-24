#pragma once
#include "modem_enumeration.hpp"
namespace porsche {
extern std::uint32_t tapi_close_cache_006ad078;
extern std::uint32_t tapi_status_cache_006ad194;
extern std::uint32_t tapi_initialize_cache_006ad1e0;
extern std::uint32_t tapi_negotiate_cache_006ad204;
extern std::uint32_t tapi_shutdown_cache_006ad2e8;
// Original 75B resolver remains an unresolved real child (stdcall, RET8).
std::int32_t __stdcall tapi_resolve_00584a60(const char*,std::uint32_t*);
}
