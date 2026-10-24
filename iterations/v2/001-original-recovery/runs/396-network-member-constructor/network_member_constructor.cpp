#include "network_member_constructor.hpp"
#include "porsche/application_heap_state.hpp"
#include "porsche/application_context_base.hpp"

namespace porsche {
void* __fastcall network_member_construct_004902c0(void* receiver,void*,const void*) {
    // ESI captures manager+28 once; child calls can replace the global manager.
    void* const pool = application_heap_address(application_context_heap_state_005e4fe8(),0x28u);
    heap_enter_005322b0(application_heap_pointer(application_heap_load(pool)));
    if (!application_heap_load(application_heap_address(pool,0x10u)))
        application_context_new_head_0059eeb0(1,0);
    const auto node = application_heap_load(application_heap_address(pool,0x10u));
    application_heap_store(application_heap_address(pool,0x10u),
        application_heap_load(application_heap_address(application_heap_pointer(node),4u)));
    heap_leave_005322c0(application_heap_pointer(application_heap_load(pool)));
    application_heap_store(receiver,node);
    application_heap_store(application_heap_pointer(node),node);
    const auto reloaded = application_heap_load(receiver);
    application_heap_store(application_heap_address(application_heap_pointer(reloaded),4u),reloaded);
    return receiver;
}
}
