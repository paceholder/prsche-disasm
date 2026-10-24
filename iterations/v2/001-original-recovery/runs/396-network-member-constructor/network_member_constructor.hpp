#pragma once
namespace porsche {
// Original thiscall: ECX receiver, one ignored opaque stack pointer, RET4/EAXself.
void* __fastcall network_member_construct_004902c0(void* receiver,void* unused_edx,const void* opaque);
}
