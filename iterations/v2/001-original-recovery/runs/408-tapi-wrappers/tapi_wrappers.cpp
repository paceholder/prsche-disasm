#include "tapi_wrappers.hpp"
#include <utility>
namespace porsche {
using U=std::uint32_t;
using I=std::int32_t;
static_assert(sizeof(void*)==4);
U tapi_close_cache_006ad078=0;
U tapi_status_cache_006ad194=0;
U tapi_initialize_cache_006ad1e0=0;
U tapi_negotiate_cache_006ad204=0;
U tapi_shutdown_cache_006ad2e8=0;
namespace {
const char close_name_005c1214[]="lineClose";
const char status_name_005c1270[]="lineGetLineDevStatus";
const char initialize_name_005c1288[]="lineInitialize";
const char negotiate_name_005c12a8[]="lineNegotiateAPIVersion";
const char shutdown_name_005c12f8[]="lineShutdown";
template<class F,class... A>
I dispatch(U& cache,const char* name,A... args){
    if(cache==0){
        const I result=tapi_resolve_00584a60(name,&cache);
        if(result!=0){cache=0xffffffffu;return result;}
    }
    const U target=cache;
    if(target==0xffffffffu)return static_cast<I>(0x80000049u);
    return reinterpret_cast<F>(static_cast<std::uintptr_t>(target))(args...);
}
}
I __stdcall modem_line_initialize_00584ce0(U* app,U instance,ModemCallback callback,const char* name,U* count){
    using F=I(__stdcall*)(U*,U,ModemCallback,const char*,U*);
    return dispatch<F>(tapi_initialize_cache_006ad1e0,initialize_name_005c1288,app,instance,callback,name,count);
}
I __stdcall modem_line_negotiate_00584d7c(U app,U device,U low,U high,U* version,void* extension){
    using F=I(__stdcall*)(U,U,U,U,U*,void*);
    return dispatch<F>(tapi_negotiate_cache_006ad204,negotiate_name_005c12a8,app,device,low,high,version,extension);
}
I __stdcall modem_line_status_00584c9d(U line,void* status){
    using F=I(__stdcall*)(U,void*);
    return dispatch<F>(tapi_status_cache_006ad194,status_name_005c1270,line,status);
}
I __stdcall modem_line_shutdown_00584eb9(U app){
    using F=I(__stdcall*)(U);
    return dispatch<F>(tapi_shutdown_cache_006ad2e8,shutdown_name_005c12f8,app);
}
I __stdcall modem_line_close_00584af2(U line){
    using F=I(__stdcall*)(U);
    return dispatch<F>(tapi_close_cache_006ad078,close_name_005c1214,line);
}
}
