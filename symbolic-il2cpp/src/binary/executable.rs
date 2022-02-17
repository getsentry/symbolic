use scroll::Pread;

use crate::utils::CSTR_CTX;

#[derive(Debug)]
pub struct Il2CppCodeGenModule<'d> {
    pub name: &'d str,
    pub method_pointers: &'d [u64], // TODO: this needs to be generic
}

impl<'d> Il2CppCodeGenModule<'d> {
    pub fn parse(buf: &'d [u8], mut offset: usize) -> anyhow::Result<Self> {
        let offset = &mut offset;

        let name_ptr = buf.gread::<u64>(offset)? as usize;
        let name = buf.pread_with::<&str>(name_ptr, CSTR_CTX)?;

        let num_methods = buf.gread::<u64>(offset)? as usize;
        let methods_ptr = buf.gread::<u64>(offset)? as usize;

        // TODO: turn this into a safe iter instead
        let method_pointers = unsafe {
            let raw_buf = buf.as_ptr().add(methods_ptr);
            std::slice::from_raw_parts(raw_buf as *const u64, num_methods)
        };

        Ok(Self {
            name,
            method_pointers,
        })
    }
}
