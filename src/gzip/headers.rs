use nom::bytes::streaming::{tag, take, take_until};
use nom::combinator::cond;
use nom::number::streaming::{le_u16, le_u32, le_u8};
use nom::sequence::{pair, terminated, tuple};
use nom::IResult;

use nom::error::ErrorKind;

use nom::bits::{bits, streaming::take as take_bits};

use crate::utils::parse_bit_to_bool;

fn parse_bitflags(i: &[u8]) -> IResult<&[u8], (bool, bool, bool, bool, bool)> {
    // TODO: add impl<I> ErrorConvert<(I, ErrorKind)> for ()
    let (i, (_pad, comment, name, extra, crc, text)) =
        bits::<_, _, ((&[u8], usize), ErrorKind), _, _>(tuple((
            take_bits(3_usize),
            parse_bit_to_bool,
            parse_bit_to_bool,
            parse_bit_to_bool,
            parse_bit_to_bool,
            parse_bit_to_bool,
        )))(i)?;
    let _: u8 = _pad;
    Ok((i, (text, crc, extra, name, comment)))
}

fn zero_terminated(i: &[u8]) -> IResult<&[u8], &[u8]> {
    terminated(take_until(&b"\0"[..]), tag(b"\0"))(i)
}

fn extra_data(i: &[u8]) -> IResult<&[u8], &[u8]> {
    let (i, len) = le_u16(i)?;
    take(len)(i)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MemberHeader {
    pub mtime: u32,
    pub os: u8,
    pub filename: Option<Vec<u8>>,
    pub fcomment: Option<Vec<u8>>,
}

impl MemberHeader {
    pub fn parse(i: &[u8]) -> IResult<&[u8], MemberHeader> {
        let (i, (_tag, _compression, bit_flags)) =
            tuple((tag(b"\x1f\x8b"), tag(b"\x08"), parse_bitflags))(i)?;
        let (i, (mtime, _xtra_flags, os)) = tuple((le_u32, le_u8, le_u8))(i)?;
        let (i, (_extra, filename, fcomment, _header_crc)) = tuple((
            cond(bit_flags.2, extra_data),
            cond(bit_flags.3, zero_terminated),
            cond(bit_flags.4, zero_terminated),
            cond(bit_flags.1, le_u16),
        ))(i)?;

        Ok((
            i,
            MemberHeader {
                mtime,
                os,
                filename: filename.map(ToOwned::to_owned),
                fcomment: fcomment.map(ToOwned::to_owned),
            },
        ))
    }
}

pub fn parse_footer(i: &[u8]) -> IResult<&[u8], (u32, u32)> {
    let (size, crc32) = pair(le_u32, le_u32)(i)?;
    Ok((size, crc32))
}
