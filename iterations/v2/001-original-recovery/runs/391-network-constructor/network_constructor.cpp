#include "network_constructor.hpp"
#include "../311-pr4-shutdown-helpers/shutdown_helpers_311.hpp"
#include "porsche/application_context_base.hpp"
#include "porsche/application_heap.hpp"
#include "porsche/application_heap_init.hpp"
#include "porsche/file_events.hpp"
#include "porsche/files.hpp"
#include "porsche/network_lifecycle.hpp"
#include "porsche/network_services.hpp"
#include <cstring>
#include <initializer_list>
namespace porsche {
// Exact contiguous original .data 005d079c..005d07c7, including padding.
const char startup_network_labels_005d079c[44]={
    'c',0,0,0,'0','.','0',0,'G','a','m','e',0,0,0,0,
    'C','T','R','L','-','A','L','T','!',0,0,0,
    'P','l','a','y','e','r',0,0,'U','D','P',0,'I','P','X',0};
std::uint32_t startup_network_timeout_005df6a8=400;
namespace {
using U=std::uint32_t;using B=std::uint8_t;
static_assert(sizeof(void*)==4,"original x86 pointer fields");
U address(const void* p){return static_cast<U>(reinterpret_cast<std::uintptr_t>(p));}
void* pointer(U value){return reinterpret_cast<void*>(static_cast<std::uintptr_t>(value));}
U get(const void* p,U offset){U value;std::memcpy(&value,static_cast<const B*>(p)+offset,4);return value;}
void put(void* p,U offset,U value){std::memcpy(static_cast<B*>(p)+offset,&value,4);}
void text(void* receiver,U offset,const char* source){std::memcpy(static_cast<B*>(receiver)+offset,source,std::strlen(source)+1);}
}
// Porsche.exe 0048fc80..0048ff2d, 686 bytes. ECX receiver, RET0, EAX=self
// even after the early destructor/free branch. No inferred object-wide reset.
void* __fastcall network_construct_0048fc80(void* receiver,void*){
    auto* object=static_cast<B*>(receiver);
    put(object,0,network_option_integer_00560a90(startup_network_labels_005d079c+40));
    put(object,4,network_option_present_005607d0(startup_network_labels_005d079c+36));
    for(U offset:{8u,12u,20u,32u,224u,248u})put(object,offset,0);
    for(U offset:{199u,252u,262u})object[offset]=0;
    // Original passes the address of an uninitialized stack byte. 004902c0
    // never reads that argument (complete raw body through RET4); preserve
    // address transport without inventing an initialized original value.
    B opaque;
    network_member_construct_004902c0(object+0x150,nullptr,&opaque);
    put(object,0x154,0);put(object,0x250,4);
    for(U offset:{0x254u,0x258u,0x25cu,0x260u,0x264u}){
        void* node=application_context_member_storage_004211d0(12,0);
        put(object,offset,address(node));put(node,0,address(node));
        node=pointer(get(object,offset));put(node,4,address(node));
    }
    put(object,0x24,0);
    object[0xc0]=0;object[0xc6]=0;object[0xbf]=0;put(object,0xe0,0);object[0xc2]=0;
    put(object,0x1c,network_lifecycle_tick_count_0048a9a0()<<5u);
    if(get(object,0)==0&&get(object,4)==0&&network_environment_0055eb70(0)==0){
        if(receiver){reinterpret_cast<ShutdownNetwork311*>(receiver)->destroy_0048ff30();application_release_0059f050(receiver);}
        return receiver;
    }
    startup_network_00628c70=receiver;
    void* member=startup_network_allocate_0059ef90(4);
    if(member)member=network_word_construct_0052df00(member,nullptr);
    put(object,0x18,address(member));
    void* lock=startup_network_allocate_0059ef90(4);
    if(lock)put(lock,0,address(heap_lock_create_005321f0()));
    put(object,0x10,address(lock));
    text(object,0x2c,startup_network_labels_005d079c+4);
    text(object,0x32,startup_network_labels_005d079c+4);
    text(object,0x58,startup_network_labels_005d079c+28);
    text(object,0xa8,startup_network_labels_005d079c);
    // Reuse the existing 005e8e50 owner. Historical file_null/network_group
    // aliases retain their separate unresolved ownership boundaries.
    text(object,0x98,application_object_heap_name_005e8e50);
    application_queue_kind(2,1);application_heap_kind(5,1);
    object[0x160]=0xbc;object[0x161]=0;object[0x162]=0;object[0x163]=0;
    startup_network_timeout_005df6a8=timer_rate_005deb48*15u;
    network_callbacks_start_00560340();network_lifecycle_clear_lists_0048e790(receiver,nullptr);
    missing_file_006afbe4=&network_missing_file_0048d9b0;
    return receiver;
}
// Frozen startup dispatcher uses a cdecl pointer argument; this is an actual
// typed adapter, never a symbol alias to the original ECX entry.
void __cdecl startup_network_construct_0048fc80(void* receiver){network_construct_0048fc80(receiver,nullptr);}
}
