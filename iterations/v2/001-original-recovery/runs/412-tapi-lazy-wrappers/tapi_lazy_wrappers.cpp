#include "tapi_lazy_wrappers.hpp"
#include <utility>
namespace porsche {
using U=std::uint32_t;
using I=std::int32_t;
static_assert(sizeof(void*)==4);
U tapi_drop_cache_006ad0b8=0;
U tapi_devcaps_cache_006ad15c=0;
U tapi_getid_cache_006ad188=0;
U tapi_open_cache_006ad20c=0;
U tapi_status_cache_006ad2bc=0;
namespace {
const char drop_name_005c1234[]="lineDrop";
const char devcaps_name_005c1240[]="lineGetDevCaps";
const char getid_name_005c1264[]="lineGetID";
const char open_name_005c12c0[]="lineOpen";
const char status_name_005c12e0[]="lineSetStatusMessages";
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
I __stdcall modem_line_get_capabilities_00584bb7(U application,U device,U version,U extension,void* buffer){
    using F=I(__stdcall*)(U,U,U,U,void*);
    return dispatch<F>(tapi_devcaps_cache_006ad15c,devcaps_name_005c1240,application,device,version,extension,buffer);
}
I __stdcall modem_tapi_line_open_00584dcd(U application,U device,U* line,U version,U extension,U instance,U privilege,U media,const void* params){
    using F=I(__stdcall*)(U,U,U*,U,U,U,U,U,const void*);
    return dispatch<F>(tapi_open_cache_006ad20c,open_name_005c12c0,application,device,line,version,extension,instance,privilege,media,params);
}
I __stdcall modem_line_get_id_00584c4c(U line,U address,U call_handle,U select,void* device_id,const char* device_class){
    using F=I(__stdcall*)(U,U,U,U,void*,const char*);
    return dispatch<F>(tapi_getid_cache_006ad188,getid_name_005c1264,line,address,call_handle,select,device_id,device_class);
}
I __stdcall modem_line_drop_00584b70(U line,const void* user_info,U size){
    using F=I(__stdcall*)(U,const void*,U);
    return dispatch<F>(tapi_drop_cache_006ad0b8,drop_name_005c1234,line,user_info,size);
}
I __stdcall modem_line_set_status_00584e72(U line,U flags,U zero){
    using F=I(__stdcall*)(U,U,U);
    return dispatch<F>(tapi_status_cache_006ad2bc,status_name_005c12e0,line,flags,zero);
}
}
