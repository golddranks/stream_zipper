use nom::Err::{Error, Failure, Incomplete};
use nom::IResult;

pub fn rejoin_str<'r, 'a: 'r, 'b: 'r>(a: &'a str, b: &'b str) -> Option<&'r str> {
    rejoin(a.as_bytes(), b.as_bytes()).map(|s| unsafe { std::str::from_utf8_unchecked(s) })
}

pub fn rejoin<'r, 'a: 'r, 'b: 'r, T>(a: &'a [T], b: &'b [T]) -> Option<&'r [T]> {
    let a_tail = a[a.len()..].as_ptr();
    if a_tail == b.as_ptr() {
        // This is
        Some(unsafe { std::slice::from_raw_parts(a.as_ptr(), a.len() + b.len()) })
    } else {
        None
    }
}

pub fn parse_bit_to_bool<'a, E>(
    input: (&'a [u8], usize),
) -> nom::IResult<(&'a [u8], usize), bool, E>
where
    E: nom::error::ParseError<(&'a [u8], usize)>,
{
    let (unparsed, bit) = nom::bits::streaming::take(1_usize)(input)?;

    Ok((
        unparsed,
        match bit {
            0 => false,
            1 => true,
            _ => panic!("Only these two bit patterns are possible with one bit."),
        },
    ))
}

pub trait NomErrorExt2<T, E1> {
    fn nom_fail<E2>(self, impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>>;
    fn nom_err<E2>(self, impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>>;
}

impl<T, E1> NomErrorExt2<T, E1> for Result<T, E1> {
    fn nom_fail<E2>(self, map: impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>> {
        match self {
            Err(e) => Err(Failure(map(e))),
            Ok(ok) => Ok(ok),
        }
    }
    fn nom_err<E2>(self, map: impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>> {
        match self {
            Err(e) => Err(Error(map(e))),
            Ok(ok) => Ok(ok),
        }
    }
}

pub trait NomErrorExt<T, E1> {
    fn map_nom_err<E2>(self, impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>>;
}

impl<T, E1> NomErrorExt<T, E1> for Result<T, nom::Err<E1>> {
    fn map_nom_err<E2>(self, map: impl Fn(E1) -> E2) -> Result<T, nom::Err<E2>> {
        match self {
            Err(nom_err) => Err(match nom_err {
                Failure(e) => Failure(map(e)),
                Error(e) => Error(map(e)),
                Incomplete(needed) => Incomplete(needed),
            }),
            Ok(ok) => Ok(ok),
        }
    }
}

pub fn fail<I, O, E>(error: E) -> IResult<I, O, E> {
    Err(Failure(error))
}

pub fn fail_wrap<T, E>(res: Result<T, E>) -> Result<T, nom::Err<E>> {
    match res {
        Ok(ok) => Ok(ok),
        Err(err) => Err(Failure(err)),
    }
}

pub fn map_err<I, O, E1, E2, P, M>(parser: P, err_map: M) -> impl Fn(I) -> IResult<I, O, E2>
where
    P: Fn(I) -> IResult<I, O, E1>,
    M: Fn(E1) -> E2,
{
    move |i: I| parser(i).map_nom_err(&err_map)
}

pub fn convert_err<I, O, E1, E2, P>(parser: P) -> impl Fn(I) -> IResult<I, O, E2>
where
    P: Fn(I) -> IResult<I, O, E1>,
    E2: From<E1>,
{
    move |i: I| parser(i).map_nom_err(From::from)
}

pub fn map<I, O1, O2, E, P, M>(parser: P, map: M) -> impl Fn(I) -> IResult<I, O2, E>
where
    P: Fn(I) -> IResult<I, O1, E>,
    M: Fn(O1) -> O2,
{
    move |i: I| match parser(i) {
        Ok((i, o)) => Ok((i, map(o))),
        Err(e) => Err(e),
    }
}

pub fn flat_map<I, O1, O2, E, P, M>(parser: P, flat_map: M) -> impl Fn(I) -> IResult<I, O2, E>
where
    P: Fn(I) -> IResult<I, O1, E>,
    M: Fn(O1) -> Result<O2, nom::Err<E>>,
{
    move |i: I| match parser(i) {
        Ok((i, o)) => match flat_map(o) {
            Ok(o) => Ok((i, o)),
            Err(e) => Err(e),
        },
        Err(e) => Err(e),
    }
}
