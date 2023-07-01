use std::cell::RefCell;
use std::mem;
use std::panic;
use std::thread;

type Error = Box<dyn std::error::Error + 'static>;

thread_local! {
    pub static LAST_ERROR: RefCell<Option<Error>> = RefCell::new(None);
}

pub trait ForeignObject: Sized {
    type RustObject;

    #[inline]
    unsafe fn from_rust(object: Self::RustObject) -> *mut Self {
        Box::into_raw(Box::new(object)) as *mut Self
    }

    #[inline]
    unsafe fn from_ref(object: &Self::RustObject) -> *const Self {
        object as *const Self::RustObject as *const Self
    }

    #[inline]
    #[allow(clippy::wrong_self_convention)]
    unsafe fn as_rust<'a>(pointer: *const Self) -> &'a Self::RustObject {
        &*(pointer as *const Self::RustObject)
    }

    #[inline]
    #[allow(clippy::wrong_self_convention)]
    unsafe fn as_rust_mut<'a>(pointer: *mut Self) -> &'a mut Self::RustObject {
        &mut *(pointer as *mut Self::RustObject)
    }

    #[inline]
    #[allow(clippy::wrong_self_convention)]
    unsafe fn into_rust(pointer: *mut Self) -> Box<Self::RustObject> {
        Box::from_raw(pointer as *mut Self::RustObject)
    }

    #[inline]
    unsafe fn drop(pointer: *mut Self) {
        if !pointer.is_null() {
            drop(Self::into_rust(pointer));
        }
    }
}

/// An error thrown by `landingpad` in place of panics.
#[derive(Debug)]
pub struct Panic(String);

impl std::fmt::Display for Panic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "symbolic panicked: {}", self.0)
    }
}

impl std::error::Error for Panic {}

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
            None => format!("thread '{thread}' panicked with '{message}'"),
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

macro_rules! ffi_fn {
    // a function that catches panics and returns a result (err goes to tls)
    (
        $(#[$attr:meta])*
        unsafe fn $name:ident($($aname:ident: $aty:ty),* $(,)*) -> Result<$rv:ty> $body:block
    ) => {
        #[no_mangle]
        $(#[$attr])*
        pub unsafe extern "C" fn $name($($aname: $aty,)*) -> $rv {
            $crate::utils::landingpad(|| $body)
        }
    };

    // a function that catches panics and returns nothing (err goes to tls)
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
    };
}
