#include "network_options.hpp"
#include "porsche/file_threads.hpp"
#include "porsche/heap.hpp"
#include <cstdint>
namespace porsche {
namespace {
using U=std::uint32_t;using B=std::uint8_t;
static_assert(sizeof(void*)==4,"original x86 pointers");
U address(const void* pointer){return static_cast<U>(reinterpret_cast<std::uintptr_t>(pointer));}
}
// Porsche.exe005607d0..0056083b (108B). The inherited name is frozen by391;
// this is UDP transport initialization and descriptor return, not option search.
U __cdecl network_option_present_005607d0(const char* name){
    if(!thread_start_lock_006a57d8)thread_start_lock_006a57d8=heap_lock_create_005321f0();
    if(network_udp_initialized_006a5b30==0&&network_udp_start_005839d0()!=0){
        const U index=network_transport_count_006abe3c;
        network_udp_initialized_006a5b30=1;
        network_transport_descriptors_006b4aa0[index]=address(network_udp_descriptor_005df6b0);
        network_transport_contexts_006b4ae0[index]=address(network_transport_context_005e1cf0);
        network_transport_count_006abe3c=index+1u;
        if(name)network_udp_descriptor_005df6b0[0]=address(name);
    }
    return network_udp_initialized_006a5b30?address(network_udp_descriptor_005df6b0+1):0;
}
// Porsche.exe00560a90..00560b33 (164B). IPX transport registration uses one
// descriptor view, including its two original indirect callback slots.
U __cdecl network_option_integer_00560a90(const char* name){
    B socket_data[0x190];
    if(!thread_start_lock_006a57d8)thread_start_lock_006a57d8=heap_lock_create_005321f0();
    if(network_ipx_initialized_006a5bb4==0&&network_wsa_startup_005894e6(0x101,socket_data)==0){
        const auto open=reinterpret_cast<NetworkTransportOpen395>(static_cast<std::uintptr_t>(network_ipx_descriptor_005df710[7]));
        const U connection=open(0,0,0);
        if(connection){
            network_ipx_initialized_006a5bb4=1;
            const auto close=reinterpret_cast<NetworkTransportClose395>(static_cast<std::uintptr_t>(network_ipx_descriptor_005df710[8]));
            close(connection);
            const U index=network_transport_count_006abe3c;
            network_transport_descriptors_006b4aa0[index]=address(network_ipx_descriptor_005df710);
            network_transport_contexts_006b4ae0[index]=address(network_transport_context_005e1cf0);
            network_transport_count_006abe3c=index+1u;
            if(name)network_ipx_descriptor_005df710[0]=address(name);
        }
        network_wsa_cleanup_005894e0();
    }
    // Final state is reloaded after child callbacks and WSACleanup.
    return network_ipx_initialized_006a5bb4?address(network_ipx_descriptor_005df710+1):0;
}
}
