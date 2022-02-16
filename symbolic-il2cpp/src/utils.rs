pub const CStrCtx: scroll::ctx::StrCtx = scroll::ctx::StrCtx::Delimiter(scroll::ctx::NULL);

pub fn get_slice<'d>(from: &'d [u8], offset: u32, size: u32) -> Result<&'d [u8], scroll::Error> {
    let offset = offset as usize;
    let size = size as usize;
    let end = offset
        .checked_add(size)
        .ok_or(scroll::Error::BadOffset(size))?;
    from.get(offset..end)
        .ok_or(scroll::Error::BadOffset(offset))
}
