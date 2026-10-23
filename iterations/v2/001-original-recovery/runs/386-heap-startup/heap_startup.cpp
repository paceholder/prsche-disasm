#include "porsche/application_heap.hpp"
#include "porsche/application_pool.hpp"

namespace porsche {
// Draft typed transport for the proven0059eafb startup handoff. The canonical
// pool provider remains PARTIAL; this adds no body/state or reinit policy.
void __cdecl application_heap_finish(std::uint32_t disk_slots,
        std::uint32_t unused,std::uint32_t operation_slots){
    (void)application_pool_initialize_005679f0(disk_slots,unused,
        static_cast<std::int32_t>(operation_slots));
}
}
