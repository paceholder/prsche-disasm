#pragma once
#include "tapi_wrappers.hpp"
namespace porsche {
// Run411 modem callback subsystem cells. No prior manifest or run owner
// exists for any of these DWORDs; Run403/410 own the neighboring 006a56xx /
// 006a57xx cells only.
extern std::uint32_t modem_call_handle_006a5778;
extern std::uint32_t modem_state_code_006a57b0;
extern std::uint32_t modem_state_flag_006a57ac;
extern std::uint32_t modem_device_config_006a466c;
extern std::uint32_t tapi_module_cache_006ad048;
extern std::uint32_t tapi_answer_cache_006ad068;
extern std::uint32_t tapi_deallocate_cache_006ad0a0;
// Host storage models the physical006a5674..006a5773 span before the existing
// Run403-owned line count. This is not an inferred original capacity: wsprintfA
// has no bound. The five proved fixed formats need at most94 bytes including
// NUL for signed32-bit arguments; wider/other writers and overlap remain HOLD.
extern char modem_log_buffer_006a5674[0x100];
std::int32_t __stdcall modem_line_answer_00584aab(std::uint32_t,std::uint32_t,std::uint32_t);
std::int32_t __stdcall modem_line_deallocate_00584b31(std::uint32_t);
std::int32_t __cdecl modem_device_config_fetch_0055e8c0();
// Unresolved real child (Run412 lazy-wrapper family): original lineGetID,
// stdcall6 / RET 0x18, proc name 005c1264, cache 006ad188.
std::int32_t __stdcall modem_line_get_id_00584c4c(std::uint32_t,std::uint32_t,std::uint32_t,std::uint32_t,void*,const char*);
}
