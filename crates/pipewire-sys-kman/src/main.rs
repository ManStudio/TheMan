use std::{
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
};

use lib_kman::libloading;
use pipewire_sys_kman::{RegistryEvents, SpaDict, SpaHook, SpaSupport};

fn main() {
    unsafe {
        let lib = libloading::Library::new(libloading::library_filename("pipewire-0.3")).unwrap();

        let lib = pipewire_sys_kman::CoreLibSys::with_library(lib).unwrap();
        println!("Loaded");

        lib.init(&[]);

        dbg!(lib.get_application_name());
        dbg!(lib.get_prgname());
        dbg!(lib.get_user_name());
        dbg!(lib.get_host_name());
        dbg!(lib.get_client_name());
        dbg!(lib.direction_reverse(pipewire_sys_kman::SpaDirection::Output));
        dbg!(lib.get_domain());

        let main_loop = (lib._pw_main_loop_new)(std::ptr::null());
        dbg!(main_loop);

        let _loop = (lib._pw_main_loop_get_loop)(main_loop);
        dbg!(_loop);

        let context = (lib._pw_context_new)(_loop, std::ptr::null_mut(), 0);
        dbg!(context);

        let core = (lib._pw_context_connect)(context, std::ptr::null_mut(), 0);
        dbg!(core);

        let registry = (lib._pw_core_get_registry)(core, 3, 0);
        dbg!(registry);

        let mut hook: SpaHook = std::mem::zeroed();
        let registry_events = RegistryEvents {
            version: 0,
            global: global,
            global_remove: global_remove,
        };

        (lib._pw_registry_add_listener)(
            registry,
            &mut hook,
            &registry_events,
            std::ptr::null_mut(),
        );

        dbg!((lib._pw_main_loop_run)(main_loop));

        (lib._pw_core_disconnect)(core);

        (lib._pw_context_destroy)(context);

        (lib._pw_main_loop_destroy)(main_loop);

        lib.deinit();
    }
}

extern "C" fn global(
    data: *mut c_void,
    id: u32,
    permissions: u32,
    ty: *const c_char,
    version: u32,
    props: *const SpaDict,
) {
    let ty = unsafe { CStr::from_ptr(ty) };
    eprintln!("Added {id},P:{permissions},V:{version} {ty:?}");
    unsafe {
        let p = props.read();

        for i in 0..p.n_items {
            let item = p.items.offset(i as isize).read();
            eprintln!(
                "\t {:?}: {:?}",
                CStr::from_ptr(item.key),
                CStr::from_ptr(item.value)
            );
        }
    }
}

extern "C" fn global_remove(data: *mut c_void, id: u32) {
    eprintln!("Removed: {id}");
}
