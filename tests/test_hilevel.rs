extern crate stream_zipper;

use std::fs;

use stream_zipper::gzip;
use stream_zipper::zip;
use stream_zipper::{start_stream, State};

#[test]
fn test_hilevel_api_zip() {
    let data_zip = fs::read("tests/assets/zip/zipped_ab.zip").unwrap();

    let mut zip_file_1 = zip::start_stream();

    let res = zip_file_1
        .read_with(&data_zip, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    let (unparsed, mut zip_file_2) = if let State::NextFile {
        unparsed_input,
        next_file,
    } = res
    {
        (unparsed_input, next_file)
    } else {
        panic!("Should have another file.");
    };

    let eof = zip_file_1
        .read_with(&[][..], |_| panic!("Should be EOF"))
        .expect("Should succeed");

    if let State::EndOfFile = eof {
    } else {
        panic!("The first file should be at EOF");
    }

    let res = zip_file_2
        .read_with(unparsed, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    if let State::EndOfFile = res {
    } else {
        panic!("Should be the final file.");
    }
}

#[test]
fn test_hilevel_api_gzip() {
    let data_zip = fs::read("tests/assets/gzip/zipped_a.txt.gz").unwrap();

    let mut zip_file_1 = gzip::start_stream();

    let res = zip_file_1
        .read_with(&data_zip, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    if let State::NeedsInputOrEof(_) = res {
    } else {
        panic!("Should be the final file.");
    }
    let eof = zip_file_1
        .read_with(&[][..], |contents| {
            panic!("Should be EOF, got: {:?}", contents)
        })
        .expect("Should succeed");

    if let State::EndOfFile = eof {
    } else {
        panic!("The first file should be at EOF.");
    }
}

#[test]
fn test_hilevel_api_agnostic() {
    let data_zip = fs::read("tests/assets/gzip/zipped_a.txt.gz").unwrap();

    // We are testing the "filetype agnostic" API, so we'll just call start_stream.
    let mut zip_file_1 = start_stream();

    let res = zip_file_1
        .read_internal_iter(&data_zip, |bytes| {
            println!("Callback called with output of len {}", bytes.len());
        })
        .expect("Should succeed");

    match res {
        State::NeedsInputOrEof(_) => (),
        state => panic!(
            "The first file should return NeedsInputOrEof but we got {:?}!",
            state
        ),
    }

    let eof = zip_file_1
        .read_internal_iter(&[][..], |_| panic!("Should be EOF"))
        .expect("Should succeed");

    if let State::EndOfFile = eof {
    } else {
        panic!("The first file should be at EOF but was {:?}!", eof);
    }

    let data_zip = fs::read("tests/assets/zip/zipped_ab.zip").unwrap();

    let mut zip_file_1 = start_stream();

    let res = zip_file_1
        .read_internal_iter(&data_zip, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    let (unparsed, mut zip_file_2) = if let State::NextFile {
        unparsed_input,
        next_file,
    } = res
    {
        (unparsed_input, next_file)
    } else {
        panic!("Should have another file.");
    };

    let eof = zip_file_1
        .read_internal_iter(&[][..], |_| panic!("Should be EOF"))
        .expect("Should succeed");

    if let State::EndOfFile = eof {
    } else {
        panic!("The first file should be at EOF");
    }

    let res = zip_file_2
        .read_internal_iter(unparsed, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    if let State::EndOfFile = res {
    } else {
        panic!("Should be the final file.");
    }
}

#[test]
#[ignore]
fn test_local_huge_zip() {
    let data_zip = fs::read("tests/assets/zip/local_huge.zip").unwrap();

    let mut zip_file_1 = zip::start_stream();

    let res = zip_file_1
        .read_with(&data_zip, |bytes| {
            println!("Yay. {}", bytes.len());
        })
        .expect("Should succeed");

    let eof = zip_file_1
        .read_with(&[][..], |_| panic!("Should be EOF"))
        .expect("Should succeed");

    if let State::EndOfFile = eof {
    } else {
        panic!("The first file should be at EOF");
    }
}

#[test]
fn test_with_zip_tsv_file() {
    use std::fs;

    let file = fs::read("tests/assets/zip/kyushu.tsv.zip").unwrap();
    let mut zipfile = stream_zipper::start_stream();

    let mut chunk_uncomp_bytes = 0;
    let mut chunk_comp_bytes = 0;

    for chunk in file.chunks(64 * 1024) {
        chunk_comp_bytes += chunk.len();

        let result = zipfile.read_internal_iter(chunk, |bytes| {
            chunk_uncomp_bytes += bytes.len();
        });

        if let Ok(State::NeedsInput) = result {
            continue;
        }

        let (unparsed_input, mut next_file) = match result {
            Ok(State::NextFile {
                unparsed_input,
                mut next_file,
            }) => (unparsed_input, next_file),
            _ => panic!("Expected the next file but got {:?}", result),
        };

        println!("Done? {:?}", unparsed_input);

        assert_eq!(next_file.name().map(|b| b), Some(&b"__MACOSX/"[..]));
        let result = next_file.read_internal_iter(unparsed_input, |input| println!("{:?}", input));

        let (unparsed_input, mut next_file) = match result {
            Ok(State::NextFile {
                unparsed_input,
                mut next_file,
            }) => (unparsed_input, next_file),
            _ => panic!("Expected the next file but got {:?}", result),
        };

        assert_eq!(
            next_file.name().map(|b| b),
            Some(&b"__MACOSX/._kyushu.tsv"[..])
        );
        let result = next_file.read_internal_iter(unparsed_input, |input| println!("{:?}", input));

        let result = match result {
            Ok(State::EndOfFile) => {
                eprintln!("Success!");
            }
            _ => panic!("Expected no more files but got {:?}", result),
        };
    }
}
