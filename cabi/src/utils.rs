use std::cell::RefCell;
use std::mem;
use std::panic;
use std::thread;

use failure::Error;

thread_local! {
    pub static LAST_ERROR: RefCell<Option<Error>> = RefCell::new(None);
}

/// An error thrown by `landingpad` in place of panics.
#[derive(Fail, Debug)]
#[fail(display = "symbolic panicked: {}", _0)]
pub struct Panic(String);

fn set_last_error(err: Error) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(err);
    });
}

pub unsafe fn set_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let thread = thread::current();
        let thread = thread.name().unwrap_or("unnamed");

        let message = match info.payload().downcast_ref::<&str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &**s,
                None => "Box<Any>",
            },
        };

        let description = match info.location() {
            Some(location) => format!(
                "thread '{}' panicked with '{}' at {}:{}",
                thread,
                message,
                location.file(),
                location.line()
            ),
            None => format!("thread '{}' panicked with '{}'", thread, message),
        };

        set_last_error(Panic(description).into())
    }));
}

pub unsafe fn landingpad<F, T>(f: F) -> T
where
    F: FnOnce() -> Result<T, Error> + panic::UnwindSafe,
{
    match panic::catch_unwind(f) {
        Ok(Ok(result)) => result,
        Ok(Err(err)) => {
            set_last_error(err);
            mem::zeroed()
        }
        Err(_) => mem::zeroed(),
    }
}

macro_rules! ffi_fn (
    // a function that catches patnics and returns a result (err goes to tls)
    (
        $(#[$attr:meta])*
        unsafe fn $name:ident($($aname:ident: $aty:ty),* $(,)*) -> Result<$rv:ty> $body:block
    ) => (
        #[no_mangle]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($aname: $aty,)*) -> $rv {
            $crate::utils::landingpad(|| $body)
        }
    );

    // a function that catches patnics and returns nothing (err goes to tls)
    (
        $(#[$attr:meta])*
        unsafe fn $name:ident($($aname:ident: $aty:ty),* $(,)*) $body:block
    ) => {
        #[no_mangle]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($aname: $aty,)*) {
            // this silences panics and stuff
            $crate::utils::landingpad(|| { $body; Ok(0 as std::os::raw::c_int) });
        }
    }
);
