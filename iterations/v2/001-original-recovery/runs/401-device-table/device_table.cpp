#include "device_table.hpp"
#include <cstring>
namespace porsche {
std::uint32_t __cdecl free_00531f90(void*);
std::uint32_t device_table_count_006a57a0=0;
std::uint32_t device_table_pointer_006a57b8=0;
std::uint32_t __cdecl network_environment_0055eb70(std::uint32_t index) {
    if (!device_table_pointer_006a57b8) device_table_enumerate_0055e4d0();
    if (static_cast<std::int32_t>(index)<0 ||
        static_cast<std::int32_t>(index)>=static_cast<std::int32_t>(device_table_count_006a57a0)) return 0;
    return device_table_pointer_006a57b8+index*12u;
}
void __cdecl device_table_clear_0055ddf0() {
    std::uint32_t table=device_table_pointer_006a57b8;
    if (!table) return;
    std::uint32_t index=0,offset=0;
    if (static_cast<std::int32_t>(device_table_count_006a57a0)>0) {
        do {
            std::uint32_t text;
            std::memcpy(&text,reinterpret_cast<const void*>(table+offset+8u),4);
            if (text) {
                free_00531f90(reinterpret_cast<void*>(text));
                table=device_table_pointer_006a57b8;
            }
            ++index;
            offset+=12u;
        } while (static_cast<std::int32_t>(index)<static_cast<std::int32_t>(device_table_count_006a57a0));
    }
    free_00531f90(reinterpret_cast<void*>(table));
    device_table_pointer_006a57b8=0;
}
}
