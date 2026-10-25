#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstring>
#include "modem_callback.hpp"
#include "porsche/fe_stream.hpp"
namespace porsche {
using U=std::uint32_t;
using I=std::int32_t;
static_assert(sizeof(void*)==4);
U modem_call_handle_006a5778=0;
U modem_state_code_006a57b0=0;
U modem_state_flag_006a57ac=0;
U modem_device_config_006a466c=0;
U tapi_module_cache_006ad048=0;
U tapi_answer_cache_006ad068=0;
U tapi_deallocate_cache_006ad0a0=0;
char modem_log_buffer_006a5674[0x100]={};
namespace {
// Original const data, byte-verified against the pinned original before any
// oracle relocation: formats 005bca34/70/a4/d8/08, class 005bcb3c, allocator
// tag 005bcbb4, module name 005c11fc, proc names 005c1208/005c1220, dispatch
// byte tables 0055e20c (12) and 0055e22c (64).
const char tapi_module_name_005c11fc[]="TAPI32.DLL";
const char line_answer_name_005c1208[]="lineAnswer";
const char line_deallocate_name_005c1220[]="lineDeallocateCall";
const char callstate_format_005bca70[]="LINE_CALLSTATE  param1: %d param2: %d  param3: %d\n";
const char callinfo_format_005bcaa4[]="LINE_CALLINFO param1: %d param2: %d  param3: %d\n";
const char devstate_format_005bcb08[]="LINE_DEVSTATE param1: %d  param2: %d  param3: %d\n";
const char reply_format_005bcad8[]="LINE_REPLY  param1: %d param2: %d  param3: %d\n";
const char unhandled_format_005bca34[]="Unhandled message: %d param1: %d  param2: %d  param3: %d\n";
const char comm_datamodem_class_005bcb3c[]="comm/datamodem";
const char tapi_varstring_name_005bcbb4[]="tapiVarString";
const unsigned char message_index_0055e20c[12]={0,1,4,4,4,4,4,2,4,4,4,3};
const unsigned char media_index_0055e22c[64]={0,0,4,4,4,4,4,1,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,2,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,3};
U word(const void* p){return static_cast<U>(reinterpret_cast<std::uintptr_t>(p));}
void* pointer(U value){return reinterpret_cast<void*>(static_cast<std::uintptr_t>(value));}
U load(const void* p){U value;std::memcpy(&value,p,4);return value;}
void store(void* p,U value){std::memcpy(p,&value,4);}
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
// 00584a60: lazy TAPI resolver, stdcall2 / RET 8. Resolves through
// LoadLibraryA("TAPI32.DLL") once, caches the module, then GetProcAddress.
// The cache-pointer output is written even when the proc is missing, but not
// on the all-ones module failure path.
I __stdcall tapi_resolve_00584a60(const char* name,U* cache){
    U module=tapi_module_cache_006ad048;
    if(module==0){
        module=word(::LoadLibraryA(tapi_module_name_005c11fc));
        tapi_module_cache_006ad048=module;
        if(module==0){module=0xffffffffu;tapi_module_cache_006ad048=0xffffffffu;}
    }
    if(module==0xffffffffu)return static_cast<I>(0x80000049u);
    const U proc=word(reinterpret_cast<const void*>(::GetProcAddress(reinterpret_cast<HMODULE>(pointer(module)),name)));
    *cache=proc;
    if(proc==0)return static_cast<I>(0x80000049u);
    return 0;
}
// 00584aab: lineAnswer lazy wrapper, stdcall3 / RET 0xC, cache 006ad068.
I __stdcall modem_line_answer_00584aab(U call_handle,U second,U third){
    using F=I(__stdcall*)(U,U,U);
    return dispatch<F>(tapi_answer_cache_006ad068,line_answer_name_005c1208,call_handle,second,third);
}
// 00584b31: lineDeallocateCall lazy wrapper, stdcall1 / RET 4, cache 006ad0a0.
I __stdcall modem_line_deallocate_00584b31(U call_handle){
    using F=I(__stdcall*)(U);
    return dispatch<F>(tapi_deallocate_cache_006ad0a0,line_deallocate_name_005c1220,call_handle);
}
// 0055e8c0: fetch the comm/datamodem device config into 006a466c, cdecl0 /
// plain RET. Allocates a tapiVarString block of 0x18 bytes, grows it to the
// reported needed size and retries; -1 on failure, 0 on success. The
// allocator token 005deb60 and call handle 006a5778 are re-read per original
// call site.
I __cdecl modem_device_config_fetch_0055e8c0(){
    void* buffer=allocate_00531ca0(tapi_varstring_name_005bcbb4,0x18,render_descriptor_flags_005deb60);
    U size=0x18;
    for(;;){
        const U call_handle=modem_call_handle_006a5778;
        store(buffer,size); // dwTotalSize before lineGetID.
        const I result=modem_line_get_id_00584c4c(0,0,call_handle,4,buffer,comm_datamodem_class_005bcb3c);
        if(result!=0){free_00531f90(buffer);return -1;}
        for(;;){
            const U needed=load(static_cast<const unsigned char*>(buffer)+4);
            const U total=load(buffer);
            if(total<needed){
                free_00531f90(buffer);
                size=needed;
                buffer=allocate_00531ca0(tapi_varstring_name_005bcbb4,static_cast<I>(needed),render_descriptor_flags_005deb60);
                break; // retry lineGetID with the grown block
            }
            const U offset=load(static_cast<const unsigned char*>(buffer)+0x14);
            modem_device_config_006a466c=load(static_cast<const unsigned char*>(buffer)+offset);
            free_00531f90(buffer);
            return 0;
        }
    }
}
// 0055df80: TAPI line callback, stdcall6 / RET 0x18. Message dispatch is
// table-driven exactly as the original byte tables direct; the default path
// logs the message number itself.
void __stdcall modem_callback_0055df80(U device,U message,U instance,U param1,U param2,U param3){
    (void)instance;
    const U index=message-1u;
    if(index<=0xbu){
        const U selector=message_index_0055e20c[index];
        if(selector!=4u){
            if(selector==0u){ // 0055e039 LINE_CALLINFO log
                ::wsprintfA(modem_log_buffer_006a5674,callinfo_format_005bcaa4,static_cast<int>(param1),static_cast<int>(param2),static_cast<int>(param3));
                modem_message_006ad43c=modem_log_buffer_006a5674;
                return;
            }
            if(selector==1u){ // 0055e06d LINE_CALLSTATE state machine
                ::wsprintfA(modem_log_buffer_006a5674,callstate_format_005bca70,static_cast<int>(param1),static_cast<int>(param2),static_cast<int>(param3));
                modem_message_006ad43c=modem_log_buffer_006a5674;
                if(param1==0x40u){modem_state_code_006a57b0=0xcu;modem_state_flag_006a57ac=1u;return;}
                if(param1<0x40u){
                    if(param1==1u){modem_line_deallocate_00584b31(modem_call_handle_006a5778);modem_state_flag_006a57ac=1u;return;}
                    if(param1!=2u)return;
                    modem_line_answer_00584aab(device,0u,0u);
                    modem_call_handle_006a5778=device;
                    return;
                }
                if(param1==0x100u){(void)modem_device_config_fetch_0055e8c0();modem_state_code_006a57b0=9u;modem_state_flag_006a57ac=1u;return;}
                if(param1!=0x4000u)return;
                const U media=param2-1u;
                if(media>0x3fu){modem_state_code_006a57b0=0xbu;modem_state_flag_006a57ac=1u;return;} // 0055e183
                switch(media_index_0055e22c[media]){
                case 0u:modem_state_code_006a57b0=0xbu;break; // 0055e12f
                case 1u:modem_state_code_006a57b0=0xeu;break; // 0055e14b
                case 2u:modem_state_code_006a57b0=0xcu;break; // 0055e0e4
                case 3u:modem_state_code_006a57b0=0xdu;break; // 0055e167
                default:modem_state_code_006a57b0=0xbu;break; // 0055e183
                }
                modem_state_flag_006a57ac=1u;
                return;
            }
            if(selector==2u){ // 0055dfa4 LINE_DEVSTATE log + line status
                ::wsprintfA(modem_log_buffer_006a5674,devstate_format_005bcb08,static_cast<int>(param1),static_cast<int>(param2),static_cast<int>(param3));
                alignas(4) unsigned char status[0x4c]; // Original sparse stack frame; not zeroed.
                modem_message_006ad43c=modem_log_buffer_006a5674;modem_line_status_00584c9d(modem_line_006a4668,status);
                return;
            }
            // 0055dfe9 LINE_REPLY log; nonzero param2 marks failure state.
            ::wsprintfA(modem_log_buffer_006a5674,reply_format_005bcad8,static_cast<int>(param1),static_cast<int>(param2),static_cast<int>(param3));
            modem_message_006ad43c=modem_log_buffer_006a5674;
            if(param2==0u)return;
            modem_state_code_006a57b0=0xfu;modem_state_flag_006a57ac=1u;
            return;
        }
    }
    // 0055e1c0: unhandled message; the message number is logged first.
    ::wsprintfA(modem_log_buffer_006a5674,unhandled_format_005bca34,static_cast<int>(message),static_cast<int>(param1),static_cast<int>(param2),static_cast<int>(param3));
    modem_message_006ad43c=modem_log_buffer_006a5674;
}
}
