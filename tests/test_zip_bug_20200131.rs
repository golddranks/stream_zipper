extern crate stream_zipper;

use std::fs;
use std::ops::Not;

use stream_zipper::{zip, State};

#[test]
fn test_bug_20200131() {
    let data_zip = fs::read("tests/assets/zip/numbers.zip").unwrap();
    let data_uncompressed = fs::read("tests/assets/uncompressed/numbers.txt").unwrap();
    let mut data_uncompressed = &data_uncompressed[..];

    let mut zip_file = zip::start_stream();

    let zip_header_len = 67;
    // Critical length is such that upon decompressing data,
    // the input buffer is going to get empty and the output buffer is going to get full
    // at the same time. See: https://github.com/Frommi/miniz_oxide/pull/68
    let critical_len = 13750 + zip_header_len;

    let mut data_zip_a = &data_zip[..critical_len];
    let mut data_zip_b = &data_zip[critical_len..];

    zip_file
        .read_with(&data_zip_a, |unzipped_bytes| {
            assert_eq!(unzipped_bytes, &data_uncompressed[..unzipped_bytes.len()]);
            data_uncompressed = &data_uncompressed[unzipped_bytes.len()..]
        })
        .expect("Should succeed");
    zip_file
        .read_with(&data_zip_b, |unzipped_bytes| {
            assert_eq!(unzipped_bytes, data_uncompressed);
        })
        .expect("Should succeed");
}
