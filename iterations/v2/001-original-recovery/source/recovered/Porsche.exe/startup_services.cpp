#include "porsche/startup_services.hpp"
#include "porsche/files.hpp"
#include <cstdint>
#include <cstring>

namespace porsche {
void __cdecl startup_diagnostic_callback_0059fbc0(const char*,...);
void* startup_network_00628c70=nullptr;

// 004a5410..004a54ac: ordered startup dispatch. Callee implementations are
// intentionally outside this bounded consumer.
void __cdecl startup_services_004a5410(){
    diagnostic_handler_005debf0=reinterpret_cast<void(__cdecl*)(const char*)>(&startup_diagnostic_callback_0059fbc0);
    startup_heap_0059e9d0(0x1800000,0x400000,0x100,0x100);
    startup_queue_0059edb0(400,0);
    startup_capacity_00565030(0x80);
    if(!startup_network_00628c70){
        if(void* network=startup_network_allocate_0059ef90(0x268))startup_network_construct_0048fc80(network);
    }
    startup_event_0055fcf0();
    startup_subsystem_00564e70();
    startup_subsystem_00564ab0();
    startup_subsystem_00564850(2);
    startup_subsystem_00536e50();
    startup_subsystem_005367b0(0,1000,1);
    startup_subsystem_004b6ff0();
    startup_subsystem_00563ad0(30,0x2000,0);
}

// 004a5c30..004a5d16: filename directory, drive/CD-ROM test, relaunch.
// Exit is a terminal recording boundary in the fixture.
void __cdecl startup_cd_relaunch_004a5c30(){
    char frame[0x278];std::memset(frame,0xcc,sizeof(frame));
    auto* filename=frame+0x70;
    auto* drive=frame+0x08;
    auto* command=frame+0x174;
    startup_name_005655f0("Porsche");
    if(startup_module_filename(nullptr,filename,0xff)){
        auto length=std::strlen(filename);
        while(length){--length;if(filename[length]=='\\'){filename[length]='\0';break;}}
    }
    const auto first=static_cast<std::int32_t>(static_cast<std::int8_t>(filename[0]));
    if(startup_drive_character_005a2c24(first)){
        startup_drive_format_005a0fbf(drive,"%c:\\",first);
        if(startup_drive_type(drive)==5){
            startup_command_format_005a0fbf(command,"%sAutorun.exe",drive);
            auto* startup=frame+0x2c;
            std::memset(startup,0,0x44);
            *reinterpret_cast<std::uint32_t*>(startup)=0x44;
            startup_create_process(nullptr,command,nullptr,nullptr,0,0,nullptr,nullptr,startup,frame+0x1c);
            startup_exit_005a246e(0);
            return;
        }
    }
}
}
