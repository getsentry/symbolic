use crate::format::{FormatError, FormatErrorKind};

/// Decodes a compressed unsigned number at the start of a byte slice, returning the number
/// and the rest of the slice in the success case.
pub(crate) fn decode_unsigned(data: &[u8]) -> Result<(u32, &[u8]), FormatError> {
    let first_byte = *data
        .first()
        .ok_or(FormatErrorKind::InvalidCompressedUnsigned)?;

    if first_byte & 0b1000_0000 == 0 {
        return Ok((first_byte as u32, &data[1..]));
    }

    if first_byte & 0b0100_0000 == 0 {
        let bytes = data
            .get(..2)
            .ok_or(FormatErrorKind::InvalidCompressedUnsigned)?;
        let num = u16::from_be_bytes(bytes.try_into().unwrap());
        let masked = num & 0b0011_1111_1111_1111;
        return Ok((masked as u32, &data[2..]));
    }

    if first_byte & 0b0010_0000 == 0 {
        let bytes = data
            .get(..4)
            .ok_or(FormatErrorKind::InvalidCompressedUnsigned)?;
        let num = u32::from_be_bytes(bytes.try_into().unwrap());
        let masked = num & 0b0001_1111_1111_1111_1111_1111_1111_1111;
        return Ok((masked, &data[4..]));
    }

    Err(FormatErrorKind::InvalidCompressedUnsigned.into())
}

/// Decodes a compressed signed number at the start of a byte slice, returning the number
/// and the rest of the slice in the success case.
pub(crate) fn decode_signed(data: &[u8]) -> Result<(i32, &[u8]), FormatError> {
    let first_byte = *data
        .first()
        .ok_or(FormatErrorKind::InvalidCompressedSigned)?;

    if first_byte & 0b1000_0000 == 0 {
        // transform `0b0abc_defg` to `0bggab_cdef`.
        let lsb = first_byte & 0b0000_0001; // lsb = 0b0000_000g
        let mut rotated = first_byte >> 1; // rotated = 0b00ab_cdef
        rotated |= lsb << 6; // rotated = 0b0gab_cdef
        rotated |= lsb << 7; // rotated = 0bggab_cdef;
        return Ok((rotated as i8 as i32, &data[1..]));
    }

    if first_byte & 0b0100_0000 == 0 {
        let bytes = data
            .get(..2)
            .ok_or(FormatErrorKind::InvalidCompressedSigned)?;
        let mut num = u16::from_be_bytes(bytes.try_into().unwrap());
        num &= 0b0011_1111_1111_1111; // clear the tag bits
        let lsb = num & 0b0000_0001;
        let mut rotated = num >> 1;
        rotated |= lsb << 13;
        rotated |= lsb << 14;
        rotated |= lsb << 15;
        return Ok((rotated as i16 as i32, &data[2..]));
    }

    if first_byte & 0b0010_0000 == 0 {
        let bytes = data
            .get(..4)
            .ok_or(FormatErrorKind::InvalidCompressedSigned)?;
        let mut num = u32::from_be_bytes(bytes.try_into().unwrap());
        num &= 0b0001_1111_1111_1111_1111_1111_1111_1111; // clear the tag bits
        let lsb = num & 0b0000_0001;
        let mut rotated = num >> 1;
        rotated |= lsb << 28;
        rotated |= lsb << 29;
        rotated |= lsb << 30;
        rotated |= lsb << 31;
        return Ok((rotated as i32, &data[4..]));
    }

    Err(FormatErrorKind::InvalidCompressedSigned.into())
}

#[cfg(test)]
mod tests {
    use super::{decode_signed, decode_unsigned};

    #[test]
    fn test_decode_unsigned() {
        let cases = [
            (&[0x03][..], 0x03),
            (&[0x7F], 0x7F),
            (&[0x80, 0x80], 0x80),
            (&[0xAE, 0x57], 0x2E57),
            (&[0xBF, 0xFF], 0x3FFF),
            (&[0xC0, 0x00, 0x40, 0x00], 0x4000),
            (&[0xDF, 0xFF, 0xFF, 0xFF], 0x1FFF_FFFF),
        ];

        for (arg, res) in cases.iter() {
            assert_eq!(decode_unsigned(arg).unwrap().0, *res);
        }
    }
    #[test]
    fn test_decode_signed() {
        let cases = [
            (&[0x01][..], -64),
            (&[0x7E], 63),
            (&[0x7B], -3),
            (&[0x80, 0x80], 64),
            (&[0x80, 0x01], -8192),
            (&[0xC0, 0x00, 0x40, 0x00], 8192),
            (&[0xDF, 0xFF, 0xFF, 0xFE], 268435455),
            (&[0xC0, 0x00, 0x00, 0x01], -268435456),
        ];

        for (arg, res) in cases.iter() {
            assert_eq!(decode_signed(arg).unwrap().0, *res);
        }
    }
}
