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

pub fn sub_slice<'d, T: scroll::ctx::SizeWith<Ctx>, Ctx>(
    from: &'d [u8],
    ctx: &Ctx,
    start_idx: u32,
    len: u32,
) -> Result<&'d [u8], scroll::Error> {
    let sizeof_elem = T::size_with(ctx);

    let start_idx = start_idx as usize;
    let start = start_idx
        .checked_mul(sizeof_elem)
        .ok_or(scroll::Error::BadOffset(start_idx))?;
    let size = (len as usize)
        .checked_mul(sizeof_elem)
        .ok_or(scroll::Error::BadOffset(len as usize))?;
    let end = start
        .checked_add(size)
        .ok_or(scroll::Error::BadOffset(size))?;

    from.get(start..end).ok_or(scroll::Error::BadOffset(start))
}
