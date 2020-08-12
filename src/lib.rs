use std::os::raw::*;
use std::io::Read;
use std::ffi::CString;
use detour::RawDetour;

use glua_sys::*;

use libloading::os::unix as unix_lib;

mod steam_decoder;

static mut lstate: Option<*mut lua_State> = None;

unsafe fn voice_hook_run(L: *mut lua_State, slot: i32, data: &[u8]) {
    static C_HOOK: &str = "hook\0";
    static C_RUN: &str = "Run\0";
    static C_VOICEDATA: &str = "VoiceData\0";

    lua_getglobal!(L, unsafe { C_HOOK.as_ptr() as *const c_char });
    lua_getfield(L, -1, unsafe { C_RUN.as_ptr() as *const c_char });

    lua_pushstring(L, unsafe { C_VOICEDATA.as_ptr() as *const c_char });

    lua_pushnumber(L, slot as f64);

    let data_i8 = unsafe { &*(data as *const _  as *const [i8]) };
    lua_pushlstring(L, data_i8.as_ptr(), data_i8.len() as _);
    // 3 arg, 0 result
    let res = lua_pcall(L, 3, 0, 0);
    lua_pop!(L, 1); // pop "hook"
}

type EngineBroadcastVoice = unsafe extern "C" fn(*mut c_void, c_int, *const u8, i64);
static mut voice_hook: Option<RawDetour> = None;
extern "C" fn voice_detour(client: *mut c_void, byte_count: c_int, data: *const u8, xuid: i64) {
    unsafe {
        let original: EngineBroadcastVoice = std::mem::transmute(voice_hook.as_ref().unwrap().trampoline());
        original(client, byte_count, data, xuid);
    }

    if !client.is_null() && byte_count > 0 && !data.is_null() {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut slice = unsafe { std::slice::from_raw_parts::<u8>(data, byte_count as usize) };

        let mut decomp = vec!();
        steam_decoder::process(&slice, &mut decomp);

        let slot = unsafe { player_slot.unwrap()(client) };
        
        unsafe {
            if let Some(ref L) = lstate {
                voice_hook_run(*L, slot, &decomp);
            }
        }
    }
}

type EngineGetPlayerSlot = unsafe extern "C" fn(*mut c_void) -> c_int;
static mut player_slot: Option<EngineGetPlayerSlot> = None;

const RTLD_LAZY: c_int = 0x00001;
const RTLD_NOLOAD: c_int = 0x00004;

#[repr(C)]
struct LinkMap32 {
    addr: u32
}

extern "C" fn enable_hook(L: *mut lua_State) -> c_int {
    unsafe {
        let path_to_lib = "bin/engine_srv.so";
        let lib = match unix_lib::Library::open(Some(path_to_lib), RTLD_LAZY | RTLD_NOLOAD) {
            Ok(lib) => lib,
            _ => {
                lua_pushboolean(L, 0);
                lua_pushstring(L, CString::new(format!("cannot find engine from {}", path_to_lib)).unwrap().as_ptr());
                return 2
            }
        };

        let ptr = lib.into_raw();
        let data: *const LinkMap32 = unsafe { ptr as *const LinkMap32 };

        let lib_bytes = std::fs::read(path_to_lib).unwrap();
        let elf = goblin::elf::Elf::parse(&lib_bytes[..]).unwrap();

        let mut n = 0;

        for syn in elf.syms.iter() {
            let name = elf.strtab.get(syn.st_name).unwrap().unwrap();

            if name == "_Z21SV_BroadcastVoiceDataP7IClientiPcx" {
                let ptr = ((*data).addr as usize + syn.st_value as usize);
                let mut hook = unsafe { RawDetour::new(ptr as *const (), voice_detour as *const ()).unwrap() };
                unsafe { hook.enable().unwrap() };
                voice_hook = Some(hook);

                n += 1;
            } else if name == "_ZNK11CBaseClient13GetPlayerSlotEv" {
                let ptr = ((*data).addr as usize + syn.st_value as usize);

                player_slot = Some(std::mem::transmute(ptr));

                n += 1;
            }

        }

        lua_pushboolean(L, if n == 2 { 1} else { 0 });
        1
    }
}

extern "C" fn disable(L: *mut lua_State) -> c_int {
    unsafe {
        if let Some(hook) = voice_hook.take() {
            if hook.is_enabled() {
                hook.disable().unwrap()
            }
        }
    }
    0
}

fn glua_setglobal(L: *mut lua_State, lua_name: &str) {
    match CString::new(lua_name) {
        Ok(cstring_name) => {
            unsafe {
                lua_setglobal!(L, cstring_name.as_ptr());
            }
        }
        Err(e) => {
            println!("Failed to create CString! {}", e);
        }
    }
}

fn glua_register_to_table(L: *mut lua_State, table_index: i32, lua_name: &str, func: unsafe extern "C" fn(*mut lua_State) -> c_int) {
    match CString::new(lua_name) {
        Ok(cstring_name) => {
            unsafe {
                lua_pushcfunction!(L, Some(func));
                lua_setfield(L, table_index, cstring_name.as_ptr());
            }
        }
        Err(e) => {
            println!("Failed to create CString! {}", e);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn gmod13_open(L: *mut lua_State) -> c_int {
    unsafe {
        lstate = Some(L);
    }

    lua_newtable!(L);
    unsafe {
        lua_pushnumber(L, 3.0);
        lua_setfield(L, -2, CString::new("Version").unwrap().as_ptr());
    }
    glua_register_to_table(L, -2, "Enable", enable_hook);
    glua_register_to_table(L, -2, "Disable", disable);
    glua_setglobal(L, "rvoicehook");

    0
}

#[no_mangle]
pub extern "C" fn gmod13_close(L: *mut lua_State) -> c_int {
    unsafe {
        lstate = None;
    }
    
    disable(L);
    0
}