extern crate stream_zipper;

use std::fs;

use stream_zipper::deflate::{self, *};
use stream_zipper::gzip::headers::*;
use stream_zipper::gzip::*;

#[test]
fn test_parsing_member_header() {
    let random_data_gzip = fs::read("tests/assets/gzip/rand_data.bin.gz").unwrap();

    let (_unparsed, parsed_header) =
        MemberHeader::parse(&random_data_gzip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        MemberHeader {
            os: 3,
            mtime: 1523430128,
            filename: Some(b"rand_data.bin".to_vec()),
            fcomment: None,
        }
    );
}

#[test]
fn test_decompression_rand_tiny() {
    let data_zip = fs::read("tests/assets/gzip/short_data.txt.gz").unwrap();
    let data_txt = fs::read("tests/assets/uncompressed/short_data.txt").unwrap();

    let (unparsed_1, parsed_header) =
        MemberHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        MemberHeader {
            os: 3,
            mtime: 1523857589,
            filename: Some(b"short_data.txt".to_vec()),
            fcomment: None,
        }
    );

    let mut def = deflate::Stream::new();
    let mut out_pos = 0;

    let unparsed_2 = if let deflate::State::Stop { unparsed_input } = def
        .inner_iter(unparsed_1, |out| {
            assert_eq!(out, &data_txt[out_pos..out_pos + out.len()]);
            out_pos += out.len();
        })
        .expect("Should be able to deflate")
    {
        unparsed_input
    } else {
        panic!("That should have been a full, complete stream!");
    };
}

#[test]
fn test_decompression_rand_small() {
    let data_zip = fs::read("tests/assets/gzip/rand_data.bin.gz").unwrap();
    let data_txt = fs::read("tests/assets/uncompressed/rand_data.bin").unwrap();

    let (unparsed_1, parsed_header) =
        MemberHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        MemberHeader {
            os: 3,
            mtime: 1523430128,
            filename: Some(b"rand_data.bin".to_vec()),
            fcomment: None,
        }
    );

    let mut def = deflate::Stream::new();
    let mut out_pos = 0;
    let deflate_state = def
        .inner_iter(unparsed_1, |out| {
            println!("Test {:?}", out.len());
            assert_eq!(out, &data_txt[out_pos..out_pos + out.len()]);
            out_pos += out.len();
        })
        .expect("Should be able to deflate");

    let unparsed_2 = if let deflate::State::Stop { unparsed_input } = deflate_state {
        unparsed_input
    } else {
        panic!("That should have been a full, complete stream!");
    };
}

#[test]
fn test_decompression_rand_big() {
    use std::cmp::min;

    let data_zip = fs::read("tests/assets/gzip/big_rand_data.bin.gz").unwrap();
    let data_txt = fs::read("tests/assets/uncompressed/big_rand_data.bin").unwrap();

    let (unparsed_1, parsed_header) =
        MemberHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        MemberHeader {
            os: 3,
            mtime: 1523596293,
            filename: Some(b"big_rand_data.bin".to_vec()),
            fcomment: None,
        }
    );

    let mut def = deflate::Stream::new();
    let mut out_pos = 0;

    let unparsed_2 = if let deflate::State::Stop { unparsed_input } = def
        .inner_iter(unparsed_1, |out| {
            assert_eq!(out, &data_txt[out_pos..out_pos + out.len()]);
            out_pos += out.len();
        })
        .expect("Should be able to deflate")
    {
        unparsed_input
    } else {
        panic!("That should have been a full, complete stream!");
    };
}
#[test]
#[ignore]
fn test_decompression_huge() {
    use std::cmp::min;

    let data_zip = fs::read("tests/assets/gzip/huge_repeat.bin.gz").unwrap();

    let (unparsed_1, parsed_header) =
        MemberHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        MemberHeader {
            os: 3,
            mtime: 1523863915,
            filename: Some(b"huge_repeat.bin".to_vec()),
            fcomment: None,
        }
    );

    let mut def = deflate::Stream::new();
    let mut out_pos = 0;

    let start = std::time::Instant::now();

    let result = def
        .inner_iter(unparsed_1, |out| {
            out_pos += out.len();
        })
        .expect("Should be able to deflate");

    let end = std::time::Instant::now() - start;

    let unparsed_2 = if let deflate::State::Stop { unparsed_input } = result {
        unparsed_input
    } else {
        panic!("That should have been a full, complete stream!");
    };

    println!(
        "Uncompressed {} bytes ({} GiB) in {}.{} secs",
        out_pos,
        out_pos / 1024 / 1024 / 1024,
        end.as_secs(),
        end.subsec_millis()
    );
}
