use scroll::Pread;

use super::BinaryCtx;

#[derive(Debug)]
pub struct CodeRegistration {
    pub codegen_modules_len: u64,
    pub codegen_modules_ptr: u64,
}

impl scroll::ctx::TryFromCtx<'_, BinaryCtx> for CodeRegistration {
    type Error = scroll::Error;

    fn try_from_ctx(from: &[u8], _ctx: BinaryCtx) -> Result<(Self, usize), Self::Error> {
        let offset = &mut 0;

        // skip: (length + pointers)
        // * reverse pinvoke wrappers
        // * generic method pointers
        // * (just pointer) generic adjustor thunks
        // * invoker pointers
        // * unresolved virtual method pointers
        // * interop data
        // * windows runtime factory
        // TODO: make this pointers-width aware
        *offset += (6 * 2 + 1) * std::mem::size_of::<u64>();

        let codegen_modules_len = from.gread(offset)?;
        let codegen_modules_ptr = from.gread(offset)?;

        Ok((
            Self {
                codegen_modules_len,
                codegen_modules_ptr,
            },
            *offset,
        ))
    }
}

impl scroll::ctx::SizeWith<BinaryCtx> for CodeRegistration {
    fn size_with(_ctx: &BinaryCtx) -> usize {
        15 * std::mem::size_of::<u64>()
    }
}
