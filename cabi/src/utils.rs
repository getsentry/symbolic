use std::mem;
use std::panic;
use std::cell::RefCell;

use symbolic_common::{ErrorKind, Error, Result};

thread_local! {
    pub static LAST_ERROR: RefCell<Option<Error>> = RefCell::new(None);
}


fn notify_err(err: Error) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(err);
    });
}

pub unsafe fn landingpad<F: FnOnce() -> Result<T> + panic::UnwindSafe, T>(
    f: F) -> T
{
    match panic::catch_unwind(f) {
        Ok(rv) => rv.map_err(|err| notify_err(err)).unwrap_or(mem::zeroed()),
        Err(err) => {
            use std::any::Any;
            let err = &*err as &Any;
            let msg = match err.downcast_ref::<&str>() {
                Some(s) => *s,
                None => {
                    match err.downcast_ref::<String>() {
                        Some(s) => &**s,
                        None => "Box<Any>",
                    }
                }
            };
            notify_err(ErrorKind::Panic(msg.to_string()).into());
            mem::zeroed()
        }
    }
}

macro_rules! ffi_fn (
    // a function that catches patnics and returns a result (err goes to tls)
    (
        $(#[$attr:meta])*
        unsafe fn $name:ident($($aname:ident: $aty:ty),*) -> Result<$rv:ty> $body:block
    ) => (
        #[no_mangle]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($aname: $aty,)*) -> $rv
        {
            $crate::utils::landingpad(|| $body)
        }
    );

    // a function that catches patnics and returns nothing (err goes to tls)
    (
        $(#[$attr:meta])*
        unsafe fn $name:ident($($aname:ident: $aty:ty),*) $body:block
    ) => {
        #[no_mangle]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($aname: $aty,)*)
        {
            // this silences panics and stuff
            $crate::utils::landingpad(|| { $body; Ok(0 as ::std::os::raw::c_int) });
        }
    }
);
