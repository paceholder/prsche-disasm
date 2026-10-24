#pragma once
#include "../391-network-constructor/network_constructor.hpp"
#include <cstdint>
namespace porsche {
// Exact original address views only. Storage, complete descriptor callback
// bindings and registry extents remain unresolved; this package defines none.
extern std::uint32_t network_udp_initialized_006a5b30;
extern std::uint32_t network_ipx_initialized_006a5bb4;
extern std::uint32_t network_transport_count_006abe3c;
extern std::uint32_t network_transport_descriptors_006b4aa0[];
extern std::uint32_t network_transport_contexts_006b4ae0[];
extern std::uint32_t network_udp_descriptor_005df6b0[];
extern std::uint32_t network_ipx_descriptor_005df710[];
extern std::uint8_t network_transport_context_005e1cf0[];
using NetworkTransportOpen395=std::uint32_t (__cdecl*)(std::uint32_t,std::uint32_t,std::uint32_t);
using NetworkTransportClose395=std::uint32_t (__cdecl*)(std::uint32_t);
std::uint32_t __cdecl network_udp_start_005839d0();
std::int32_t __stdcall network_wsa_startup_005894e6(std::uint16_t,void*);
std::int32_t __stdcall network_wsa_cleanup_005894e0();
}
