#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include "file_backends.hpp"
#include "porsche/files.hpp"
namespace porsche {
static_assert(sizeof(void*)==4 && sizeof(PhysicalFile)==32 && offsetof(PhysicalFile,view)==20);
std::uint32_t __cdecl file_backend_00591980(void* name) {
    ::SetLastError(0);
    return static_cast<std::uint32_t>(::DeleteFileA(static_cast<const char*>(name)));
}
std::uint32_t __cdecl file_backend_00592490(void* encoded) {
    const auto index=~static_cast<std::uint32_t>(reinterpret_cast<std::uintptr_t>(encoded));
    if (physical_files_006af084 && static_cast<std::int32_t>(index)>=0 &&
        static_cast<std::int32_t>(index)<physical_count_006af080) {
        const auto* file=physical_files_006af084+index;
        if (file->active) return file->view;
    }
    ::SetLastError(6);
    return 0;
}
}
