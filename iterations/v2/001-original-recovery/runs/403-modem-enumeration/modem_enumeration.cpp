#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstring>
#include "modem_enumeration.hpp"
#include "porsche/fe_stream.hpp"
#include "porsche/formatter_bootstrap.hpp"
#include "porsche/startup_input.hpp"
#include "porsche/file_wait.hpp"
namespace porsche {
std::uint32_t modem_selected_device_005df478=0xffffffffu;
std::uint32_t modem_line_006a4668=0;
std::uint32_t modem_application_006a5670=0;
std::uint32_t modem_line_count_006a5774=0;
std::uint32_t modem_open_mode_006a579c=0;
std::uint32_t modem_api_version_006a57a4=0;
std::uint32_t modem_capabilities_006a57b4=0;
const char* modem_message_006ad43c=nullptr;
const char modem_shutdown_error_005bc98c[21]="lineShutdown failed\n";
namespace {
using U=std::uint32_t;
U word(const void* p){return static_cast<U>(reinterpret_cast<std::uintptr_t>(p));}
void* pointer(U value){return reinterpret_cast<void*>(static_cast<std::uintptr_t>(value));}
U load(U address){U value;std::memcpy(&value,pointer(address),4);return value;}
void store(U address,U value){std::memcpy(pointer(address),&value,4);}
void byte(U address,unsigned char value){std::memcpy(pointer(address),&value,1);}
}
std::uint32_t __cdecl device_table_enumerate_0055e4d0(){
    alignas(4) unsigned char local[0x5c]; // Exact sparse original stack: not zeroed.
    device_table_count_006a57a0=0;
    const auto instance=static_cast<U>(::GetWindowLongA(static_cast<HWND>(startup_window_handle_00557460()),-6));
    if(modem_line_initialize_00584ce0(&modem_application_006a5670,instance,&modem_callback_0055df80,nullptr,&modem_line_count_006a5774))return 0;
    if(!modem_line_count_006a5774){
        file_sleep_0055f740(500);
        if(modem_line_shutdown_00584eb9(modem_application_006a5670))modem_message_006ad43c=modem_shutdown_error_005bc98c;
        return 0;
    }
    device_table_clear_0055ddf0();
    const U table=word(allocate_00531ca0("modemBuf",240,render_descriptor_flags_005deb60));
    device_table_pointer_006a57b8=table;
    for(U i=0;i!=20;++i){store(table+i*12u+8u,0);store(table+i*12u,0);}
    for(U device=0;device<modem_line_count_006a5774;++device){
        if(modem_line_negotiate_00584d7c(modem_application_006a5670,device,0x10004,0x10004,&modem_api_version_006a57a4,local))continue;
        if(modem_device_capabilities_0055e6e0(device))continue;
        if(!(static_cast<const unsigned char*>(pointer(modem_capabilities_006a57b4))[0x3c]&0x10))continue;
        modem_selected_device_005df478=device;
        modem_open_mode_006a579c=0;
        if(!modem_line_open_0055e270())continue;
        store(word(local+16),76);
        if(!modem_line_status_00584c9d(modem_line_006a4668,local+16)){
            auto* const text=static_cast<char*>(allocate_00531ca0("tapiname",static_cast<std::int32_t>(load(modem_capabilities_006a57b4+0x20u)+1u),render_descriptor_flags_005deb60));
            store(device_table_pointer_006a57b8+device_table_count_006a57a0*12u+8u,word(text));
            const U caps=modem_capabilities_006a57b4;
            formatter_error_copy_n_005a28a0(text,static_cast<const char*>(pointer(caps+load(caps+0x24u))),load(caps+0x20u));
            const U count=device_table_count_006a57a0;
            const U slot=device_table_pointer_006a57b8+count*12u;
            const U length=load(modem_capabilities_006a57b4+0x20u);
            const U name=load(slot+8u);
            store(slot,device);
            byte(name+length,0);
            const U flags=load(slot+4u);
            store(slot+4u,(flags&~1u)|(load(word(local+16)+20u)==0?1u:0u));
            device_table_count_006a57a0=count+1u;
        }
        modem_line_close_00584af2(modem_line_006a4668);
    }
    file_sleep_0055f740(500);
    if(modem_line_shutdown_00584eb9(modem_application_006a5670))modem_message_006ad43c=modem_shutdown_error_005bc98c;
    return device_table_count_006a57a0;
}
}
