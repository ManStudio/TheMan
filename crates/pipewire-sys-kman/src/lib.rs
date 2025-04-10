use std::{
    ffi::{CStr, c_char, c_int, c_void},
    fmt::Pointer,
    ptr::NonNull,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum SpaDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaSupport {
    pub ty: *const c_char,
    pub data: *mut c_void,
}

impl SpaSupport {
    pub fn ty(&self) -> Option<&CStr> {
        if self.ty.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(self.ty) })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaDict {
    pub flags: u32,
    pub n_items: u32,
    pub items: *const SpaDictItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaDictItem {
    pub key: *const c_char,
    pub value: *const c_char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaHandle {
    version: u32,
    get_interface: extern "C" fn(*mut SpaHandle, ty: *const c_char, iface: *mut c_void) -> c_int,
    clear: extern "C" fn(*mut SpaHandle) -> c_int,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainLoop {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct MainLoopEvents {
    version: u32,
    destroy: extern "C" fn(*mut c_void),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaHook {
    link: SpaList,
    cb: SpaCallbacks,
    removed: Option<extern "C" fn(*mut SpaHook)>,
    _priv: *mut c_void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaList {
    next: *mut SpaList,
    prev: *mut SpaList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaCallbacks {
    funcs: *const c_void,
    data: *mut c_void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Loop {
    system: *mut SpaSystem,
    _loop: *mut SpaLoop,
    control: *mut SpaLoopControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaInterface {
    ty: *const c_char,
    version: u32,
    cb: SpaCallbacks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaSystem {
    iface: SpaInterface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaLoopControl {
    iface: SpaInterface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SpaLoop {
    iface: SpaInterface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplClient {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplGlobal {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplNode {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proxy {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Core {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registry {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataLoop {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkQueue {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemPool {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Global {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ContextEvents {
    version: u32,
    destroy: extern "C" fn(data: *mut c_void),
    free: extern "C" fn(data: *mut c_void),
    check_access: extern "C" fn(data: *mut c_void, client: *mut ImplClient),
    global_added: extern "C" fn(data: *mut c_void, global: *mut ImplGlobal),
    global_removed: extern "C" fn(data: *mut c_void, global: *mut ImplGlobal),
    driver_added: extern "C" fn(data: *mut c_void, node: *mut ImplNode),
    driver_removed: extern "C" fn(data: *mut c_void, node: *mut ImplNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ExportType {
    link: SpaList,
    ty: *const c_char,
    func: extern "C" fn(
        *mut Core,
        *const c_char,
        *const SpaDict,
        object: *mut c_void,
        user_data_size: usize,
    ) -> *mut Proxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Properties {
    dict: SpaDict,
    flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct RegistryEvents {
    pub version: u32,
    pub global: extern "C" fn(
        *mut c_void,
        id: u32,
        permissions: u32,
        ty: *const c_char,
        version: u32,
        props: *const SpaDict,
    ),
    pub global_remove: extern "C" fn(*mut c_void, id: u32),
}

lib_kman::make_lib! {
  pub struct CoreLibSys {
    // FROM: https://docs.pipewire.org/group__pw__pipewire.html
    pub(crate) pw_init: extern "C" fn(argc: c_int, argv: *const *const c_char);
    pub(crate) pw_deinit: extern "C" fn();
    pub(crate) pw_debug_is_category_enabled: extern "C" fn(name: *const c_char) -> bool;
    pub(crate) pw_get_application_name: extern "C" fn() -> *const c_char;
    pub(crate) pw_get_prgname: extern "C" fn() -> *const c_char;
    pub(crate) pw_get_user_name: extern "C" fn() -> *const c_char;
    pub(crate) pw_get_host_name: extern "C" fn() -> *const c_char;
    pub(crate) pw_get_client_name: extern "C" fn() -> *const c_char;
    pub(crate) pw_check_option: extern "C" fn(option: *const c_char, value: *const c_char) -> bool;
    pub(crate) pw_direction_reverse: extern "C" fn(SpaDirection) -> SpaDirection;
    pub(crate) pw_set_domain: extern "C" fn(domain: *const c_char) -> c_int;
    pub(crate) pw_get_domain: extern "C" fn() -> *const c_char;
    pub(crate) pw_get_support: extern "C" fn(supports: *mut SpaSupport, max: u32) -> u32;
    pub(crate) pw_load_spa_handle: extern "C" fn(lib: *const c_char, factory_name: *const c_char, info: *const SpaDict, n_support: u32, *const SpaSupport) -> *mut SpaHandle;
    pub(crate) pw_unload_spa_handle: extern "C" fn(*mut SpaHandle) -> c_int;

    // FROM: https://docs.pipewire.org/group__pw__main__loop.html
    pub pw_main_loop_new: extern "C" fn(props: *const SpaDict) -> *mut MainLoop;
    pub pw_main_loop_add_listener: extern "C" fn(_loop: *mut MainLoop, listener: *mut SpaHook, events: *const MainLoopEvents, data: *mut c_void);
    pub pw_main_loop_get_loop: extern "C" fn(_loop: *mut MainLoop) -> *mut Loop;
    pub pw_main_loop_destroy: extern "C" fn(_loop: *mut MainLoop);
    pub pw_main_loop_run: extern "C" fn(_loop: *mut MainLoop) -> c_int;
    pub pw_main_loop_quit: extern "C" fn(_loop: *mut MainLoop) -> c_int;

    // FROM: https://docs.pipewire.org/group__pw__context.html
    pub pw_context_new: extern "C" fn(*mut Loop, props: *mut Properties, user_data_size: usize) -> *mut Context;
    pub pw_context_destroy: extern "C" fn(*mut Context);
    pub pw_context_get_user_data: extern "C" fn(*mut Context) -> *mut c_void;
    pub pw_context_add_listener: extern "C" fn(*mut Context, listener: *mut SpaHook, events: *const ContextEvents, *mut c_void);
    pub pw_context_get_properties: extern "C" fn(*mut Context) -> *const Properties;
    pub pw_context_update_properties: extern "C" fn(*mut Context, dict: *const SpaDict) -> c_int;
    pub pw_context_get_conf_section: extern "C" fn(*mut Context, section: *const c_char) -> *const c_char;
    pub pw_context_parse_conf_section: extern "C" fn(*mut Context, conf: *mut Properties, section: *const c_char) -> c_int;
    pub pw_context_conf_update_props: extern "C" fn(*mut Context, section: *const c_char, props: *mut Properties) -> c_int;
    pub pw_context_conf_section_for_each: extern "C" fn(*mut Context, section: *const c_char, callback: extern "C" fn(data: *mut c_void, location: *const c_char, section: *const c_char, str: *const c_char, len: usize) -> c_int, data: *mut c_void) -> c_int;
    pub pw_context_conf_section_match_rules: extern "C" fn(*mut Context, section: *const c_char, props: *const SpaDict, callback: extern "C" fn(data: *mut c_void, location: *const c_char, action: *const c_char, str: *const c_char, len: usize) -> c_int, data: *mut c_void) -> c_int;
    pub pw_context_get_support: extern "C" fn(*mut Context, n_support: *mut u32) -> *const SpaSupport;
    pub pw_context_get_main_loop: extern "C" fn(*mut Context) -> *mut Loop;
    pub pw_context_get_data_loop: extern "C" fn(*mut Context) -> *mut DataLoop;
    pub pw_context_acquire_loop: extern "C" fn(*mut Context, props: *const SpaDict) -> *mut Loop;
    pub pw_context_release_loop: extern "C" fn(*mut Context, _loop: *mut Loop);
    pub pw_context_get_work_queue: extern "C" fn(*mut Context) -> *mut WorkQueue;
    pub pw_context_get_mempool: extern "C" fn(*mut Context) -> *mut MemPool;
    pub pw_context_for_each_global: extern "C" fn(*mut Context, callback: extern "C" fn(data: *mut c_void, global: *mut Global), data: *mut c_void) -> c_int;
    pub pw_context_find_global: extern "C" fn(*mut Context, id: u32) -> *mut Global;
    pub pw_context_add_spa_lib: extern "C" fn(*mut Context, factory_regex: *const c_char, lib: *const c_char) -> c_int;
    pub pw_context_find_spa_lib: extern "C" fn(*mut Context, factory_name: *const c_char) -> *const c_char;
    pub pw_context_load_spa_handle: extern "C" fn(*mut Context, factory_name: *const c_char, info: *const SpaDict) -> *mut SpaHandle;
    pub pw_context_register_export_type: extern "C" fn(*mut Context, ty: *mut ExportType) -> c_int;
    pub pw_context_find_export_type: extern "C" fn(*mut Context, ty_: *const c_char) -> *const ExportType;
    pub pw_context_set_object: extern "C" fn(*mut Context, ty: *const c_char, value: *mut c_void);
    pub pw_context_get_object: extern "C" fn(*mut Context, ty: *const c_char) -> *mut c_void;

    // FROM: https://docs.pipewire.org/group__pw__core.html
    pub pw_core_get_registry: extern "C" fn (*mut Core, version: u32, user_data_size: usize) -> *mut Registry;
    pub pw_context_connect: extern "C" fn(*mut Context, properties: *mut Properties, user_data_size: usize) -> *mut Core;
    pub pw_core_disconnect: extern "C" fn(*mut Core) -> i32;

    // FROM: https://docs.pipewire.org/group__pw__registry.html
    pub pw_registry_add_listener: extern "C" fn(*mut Registry, listener: *mut SpaHook, events: *const RegistryEvents, data: *mut c_void) -> i32;
  }
}

impl CoreLibSys {
    pub fn init(&self, args: &[&CStr]) {
        (self._pw_init)(args.len() as i32, args.as_ptr() as *const _);
    }

    pub fn deinit(&self) {
        (self._pw_deinit)()
    }

    pub fn debug_is_category_enabled(&self, name: &CStr) -> bool {
        (self._pw_debug_is_category_enabled)(name.as_ptr())
    }

    pub fn get_application_name<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_application_name)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn get_prgname<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_prgname)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn get_user_name<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_user_name)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn get_host_name<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_host_name)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn get_client_name<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_client_name)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn check_option(&self, option: &CStr, value: &CStr) -> bool {
        (self._pw_check_option)(option.as_ptr(), value.as_ptr())
    }

    pub fn direction_reverse(&self, direction: SpaDirection) -> SpaDirection {
        (self._pw_direction_reverse)(direction)
    }

    pub fn set_domain(&self, domain: &CStr) -> c_int {
        (self._pw_set_domain)(domain.as_ptr())
    }

    pub fn get_domain<'a>(&self) -> Option<&'a CStr> {
        let ptr = (self._pw_get_domain)();
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) })
        }
    }

    pub fn get_support(&self, supports: &mut [SpaSupport]) -> u32 {
        (self._pw_get_support)(supports.as_mut_ptr(), supports.len() as u32)
    }
}
