extern crate stream_zipper;

use std::fs;

use stream_zipper::zip;

#[test]
fn test_bug_20210407() {
    // This file has a non-standard extra field, which we should ignore
    let data_zip = fs::read("tests/assets/zip/test_zip_bug_20210407.zip").unwrap();

    let mut zip_file = zip::start_stream();

    zip_file
        .read_with(&data_zip, |_| ())
        .expect("Should succeed");
}
