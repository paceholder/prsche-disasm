#include "../391-network-constructor/network_constructor.hpp"
#include "porsche/network_services.hpp"
#include <cstring>
namespace porsche {
namespace {
using U=std::uint32_t;using B=std::uint8_t;
static_assert(sizeof(void*)==4,"original x86 pointers");
U word(const void* p,U offset){U value;std::memcpy(&value,static_cast<const B*>(p)+offset,4);return value;}
}
// Porsche.exe0052df00..0052df08: ECX receiver, EAX=self, RET0.
void* __fastcall network_word_construct_0052df00(void* receiver,void*){
    const U zero=0;std::memcpy(receiver,&zero,4);return receiver;
}
// Porsche.exe0048d9b0..0048d9e8: cdecl filename/error callback, RET0.
// Both pointer arguments below are borrowed raw tokens. The callback itself
// never reads filename or sender memory, including NULL/invalid pointer values.
void __cdecl network_missing_file_0048d9b0(const char* filename,std::int32_t error){
    void* network=startup_network_00628c70;
    if(!network||word(network,8)==0)return;
    if(static_cast<const B*>(network)[0xbf]!=0)return;
    void* sender=reinterpret_cast<void*>(static_cast<std::uintptr_t>(word(network,0x0c)));
    const U filename_token=static_cast<U>(reinterpret_cast<std::uintptr_t>(filename));
    application_network_message_0048c870(sender,nullptr,0xbbu,
        static_cast<U>(error),0x80000000u,filename_token,0);
}
}
