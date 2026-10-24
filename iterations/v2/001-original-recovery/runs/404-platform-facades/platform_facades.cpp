#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include "porsche/files.hpp"
#include "porsche/startup_services.hpp"
#include "porsche/formatter_crt_runtime.hpp"
static_assert(sizeof(void*)==4,"original x86 Win32 ABI");
static_assert(sizeof(STARTUPINFOA)==0x44&&sizeof(PROCESS_INFORMATION)==0x10);
namespace porsche {
// Existing canonical stdcall provider owns KERNEL32!SetLastError. This is the
// cdecl adaptation required by the recovered files.cpp consumer, not a new
// original body or a mutable disk-platform callback indirection.
void __cdecl file_set_last_error(std::uint32_t error){formatter_crt_set_last_error(error);}
std::uint32_t __stdcall startup_drive_type(const char* root){return ::GetDriveTypeA(root);}
std::uint32_t __stdcall startup_create_process(const char* application,char* command,
    void* process_attributes,void* thread_attributes,std::int32_t inherit_handles,
    std::uint32_t creation_flags,void* environment,const char* directory,void* startup,void* process){
    return static_cast<std::uint32_t>(::CreateProcessA(application,command,
        static_cast<LPSECURITY_ATTRIBUTES>(process_attributes),static_cast<LPSECURITY_ATTRIBUTES>(thread_attributes),
        inherit_handles,creation_flags,environment,directory,
        static_cast<LPSTARTUPINFOA>(startup),static_cast<LPPROCESS_INFORMATION>(process)));
}
}
