pub use libloading;
pub use paste;

#[macro_export]
macro_rules! make_lib {
    ($sv:vis struct $name: ident { $($sym_v:vis $sym_name: ident: $sym_type: ty;)* } ) => {
        $crate::paste::paste!{
            $sv struct $name {
                library: Box<$crate::libloading::Library>,

                $($sym_v [<_ $sym_name>]: $crate::libloading::Symbol<'static, $sym_type>),*
            }

            impl $name{
                pub fn with_library(library: $crate::libloading::Library) -> Result<Self, $crate::libloading::Error>{
                    let library = Box::new(library);

                    let lib: &'static $crate::libloading::Library = unsafe{ &*(&*library as *const _)};

                    Ok(unsafe{Self{
                        library,

                        $(
                            [<_ $sym_name>]: lib.get(concat!(stringify!($sym_name), '\0').as_bytes())?,
                        )*
                    }})
                }
            }
        }
    };
}
