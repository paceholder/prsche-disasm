#include "modem_caps_open.hpp"
#include "porsche/fe_stream.hpp"
#include <cstring>
namespace porsche {
std::uint32_t modem_configuration_006a57a8=0;
std::uint32_t __cdecl free_00531f90(void*);
namespace {
using U=std::uint32_t;
U word(const void* p){return static_cast<U>(reinterpret_cast<std::uintptr_t>(p));}
void* pointer(U value){return reinterpret_cast<void*>(static_cast<std::uintptr_t>(value));}
U load(U address){U value;std::memcpy(&value,pointer(address),4);return value;}
void store(U address,U value){std::memcpy(pointer(address),&value,4);}
}
void __cdecl modem_capabilities_clear_0055de50(){
    const U buffer=modem_capabilities_006a57b4;
    if(buffer){
        free_00531f90(pointer(buffer));
        modem_capabilities_006a57b4=0;
    }
}
std::int32_t __cdecl modem_device_capabilities_0055e6e0(U device){
    modem_capabilities_clear_0055de50();
    U bytes=240;
    for(;;){
        void* const buffer=allocate_00531ca0("tapiLineDevCaps",static_cast<std::int32_t>(bytes),render_descriptor_flags_005deb60);
        const U version=modem_api_version_006a57a4;
        const U application=modem_application_006a5670;
        modem_capabilities_006a57b4=word(buffer);
        store(word(buffer),bytes);
        const auto result=modem_line_get_capabilities_00584bb7(application,device,version,0,buffer);
        if(result){modem_capabilities_clear_0055de50();return result;}
        const U current=modem_capabilities_006a57b4;
        const U needed=load(current+4u);
        const U total=load(current);
        if(total>=needed)return result;
        modem_capabilities_clear_0055de50();
        bytes=needed;
    }
}
std::int32_t __cdecl modem_line_open_0055e270(){
    const U first_device=modem_selected_device_005df478;
    unsigned char extension[16]; // Original output-only local remains opaque.
    if(first_device==0xffffffffu)return 0;
    if(modem_line_negotiate_00584d7c(modem_application_006a5670,first_device,0x10004,0x10004,&modem_api_version_006a57a4,extension))return 0;
    if(modem_device_capabilities_0055e6e0(modem_selected_device_005df478))return 0;
    const U privilege=modem_open_mode_006a579c?4u:1u;
    const U version=modem_api_version_006a57a4;
    const U device=modem_selected_device_005df478;
    const U application=modem_application_006a5670;
    if(modem_tapi_line_open_00584dcd(application,device,&modem_line_006a4668,version,0,0,privilege,0x10,nullptr))return 0;
    if(modem_configuration_006a57a8){
        (void)modem_line_configure_0055e340(modem_line_006a4668);
        const U capabilities=modem_capabilities_006a57b4;
        const U line=modem_line_006a4668;
        (void)modem_line_set_status_00584e72(line,load(capabilities+0x80u),0);
    }
    return 1;
}
}
