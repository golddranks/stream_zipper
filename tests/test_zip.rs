extern crate stream_zipper;

use std::fs;

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use stream_zipper::deflate;
use stream_zipper::zip::headers::*;
use stream_zipper::zip::*;

pub fn generate_systime(
    year: u32,
    month: u32,
    day: u32,
    hours: u32,
    minutes: u32,
    seconds: u32,
) -> SystemTime {
    const SEC: Duration = Duration::from_secs(1);
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    // The accumulated days in a year, at month granularity
    const DAY_OF_YEAR: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    // Leap year handling only implemented for this year range for tests:
    assert!(year >= 1972);
    assert!(year <= 2099);
    let month = (month - 1) as usize;
    let day = day - 1;
    let years_since = year - 1970;
    let is_after_leap = (year % 4) > 0 || month > 1;
    let leap_days_since = ((years_since - 2) / 4) + is_after_leap as u32;
    let days_since = 365 * years_since + DAY_OF_YEAR[month] as u32 + day + leap_days_since;
    let mins_since = hours * 60 + minutes;
    let secs_since = mins_since * 60 + seconds;
    UNIX_EPOCH + DAY * days_since + SEC * secs_since
}

#[test]
fn test_parse_msdos_datetime() {
    fn test(input: u32, year: u32, month: u32, day: u32, hours: u32, minutes: u32, seconds: u32) {
        let bytes = input.to_le_bytes();
        let res = datetime::parse_msdos_datetime(&bytes).unwrap();
        assert_eq!(
            res,
            (
                &b""[..],
                generate_systime(year, month, day, hours, minutes, seconds)
            )
        );
    }
    test(0x4c8a05bd, 2018, 4, 10, 0, 45, 58);
    test(0x4c8d697d, 2018, 4, 13, 13, 11, 58);
    test(0x4c90922f, 2018, 4, 16, 18, 17, 30);
    test(0x4c90923d, 2018, 4, 16, 18, 17, 58);
    test(0x4ccc6f77, 2018, 6, 12, 13, 59, 46);
    test(0x4ccc70c5, 2018, 6, 12, 14, 6, 10);
}

#[test]
fn test_parsing_local_header() {
    let random_data_zip = fs::read("tests/assets/zip/rand_data.bin.zip").unwrap();

    let (_unparsed, parsed_header) =
        LocalFileHeader::parse(&random_data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 10, 0, 45, 58),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"rand_data.bin"[..].into(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [222, 138, 203, 90, 182, 138, 203, 90, 245, 1, 20, 0][..].into(),
            )],
        }
    );
}

#[test]
fn test_inner_iterion_rand_small() {
    let data_zip = fs::read("tests/assets/zip/rand_data.bin.zip").unwrap();
    let data_txt = fs::read("tests/assets/uncompressed/rand_data.bin").unwrap();

    let (unparsed_1, parsed_header) =
        LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 10, 0, 45, 58),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"rand_data.bin"[..].into(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [222, 138, 203, 90, 182, 138, 203, 90, 245, 1, 20, 0][..].into(),
            )],
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

    let (unparsed_3, parsed_data_desc) =
        DataDescriptor::parse_zip(&unparsed_2).expect("Should be able to parse");

    assert_eq!(
        parsed_data_desc,
        DataDescriptor {
            tag: true,
            crc_32: 144611253,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u64,
            uncompressed_size: data_txt.len() as u64,
        }
    );

    let (unparsed_4, parsed_data_desc) =
        CentralDirHeader::parse(&unparsed_3).expect("Should be able to parse");
    let (unparsed_5, parsed_central_dir_end) =
        CentralDirEnd::parse(&unparsed_4).expect("Should be able to parse");

    assert!(unparsed_5.is_empty());
}

#[test]
fn test_inner_iterion_rand_big() {
    let data_zip = fs::read("tests/assets/zip/big_rand_data.bin.zip").unwrap();
    let data_txt = fs::read("tests/assets/uncompressed/big_rand_data.bin").unwrap();

    let (unparsed_1, parsed_header) =
        LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 13, 13, 11, 58),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"big_rand_data.bin"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [31, 46, 208, 90, 14, 46, 208, 90, 245, 1, 20, 0][..].to_vec(),
            )],
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

    let (unparsed_3, parsed_data_desc) =
        DataDescriptor::parse_zip(&unparsed_2).expect("Should be able to parse");

    assert_eq!(
        parsed_data_desc,
        DataDescriptor {
            tag: true,
            crc_32: 281740228,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u64,
            uncompressed_size: data_txt.len() as u64,
        }
    );

    let (unparsed_4, parsed_central_dir_entry) =
        CentralDirHeader::parse(&unparsed_3).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_entry,
        CentralDirHeader {
            version_needed: 20,
            version_made_by: (21, VersionMadeBy::Unix),
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod_time: 27005,
            last_mod_date: 19597,
            crc_32: 281740228,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u32,
            uncompressed_size: (out_pos as u64 % (1024 * 1024 * 1024 * 4)) as u32,
            disk_no_start: 0,
            int_file_attrib: 0,
            ext_file_attrib: 2175025152,
            rel_offset_loc_header: 0,
            filename: b"big_rand_data.bin"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [31, 46, 208, 90, 14, 46, 208, 90][..].to_vec(),
            )],
            comment: b""[..].to_vec(),
        }
    );

    let (unparsed_5, parsed_central_dir_end) =
        CentralDirEnd::parse(&unparsed_4).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_end,
        CentralDirEnd {
            this_disk_num: 0,
            central_dir_start_disk_num: 0,
            central_dir_num_entries_this_disk: 1,
            central_dir_num_entries_total: 1,
            central_dir_size: 75,
            central_dir_start_offset: 10489039,
            comment: b""[..].to_vec(),
        }
    );

    assert!(unparsed_5.is_empty());
}

#[test]
#[ignore]
fn test_inner_iterion_huge() {
    let data_zip = fs::read("tests/assets/zip/huge_repeat.bin.zip").unwrap();

    let (unparsed_1, parsed_header) =
        LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 16, 18, 17, 30),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"huge_repeat.bin"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [219, 81, 212, 90, 107, 81, 212, 90, 53, 50, 50, 131][..].to_vec(),
            )],
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

    let (unparsed_3, parsed_data_desc) =
        DataDescriptor::parse_zip(&unparsed_2).expect("Should be able to parse");

    assert_eq!(
        parsed_data_desc,
        DataDescriptor {
            tag: true,
            crc_32: 423114947,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u64,
            uncompressed_size: out_pos as u64 % (1024 * 1024 * 1024 * 4),
        }
    );

    println!(
        "Uncompressed {} bytes ({} GiB) in {}.{} secs",
        out_pos,
        out_pos / 1024 / 1024 / 1024,
        end.as_secs(),
        end.subsec_millis()
    );

    let (unparsed_4, parsed_central_dir_entry) =
        CentralDirHeader::parse(&unparsed_3).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_entry,
        CentralDirHeader {
            version_needed: 20,
            version_made_by: (21, VersionMadeBy::Unix),
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod_time: 33788,
            last_mod_date: 19600,
            crc_32: 423114947,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u32,
            uncompressed_size: (out_pos as u64 % (1024 * 1024 * 1024 * 4)) as u32,
            disk_no_start: 0,
            int_file_attrib: 0,
            ext_file_attrib: 2172665856,
            rel_offset_loc_header: 0,
            filename: b"huge_repeat.bin"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [219, 81, 212, 90, 107, 81, 212, 90][..].to_vec(),
            )],
            comment: b""[..].to_vec(),
        }
    );

    let (unparsed_5, parsed_central_dir_end) =
        CentralDirEnd::parse(&unparsed_4).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_end,
        CentralDirEnd {
            this_disk_num: 0,
            central_dir_start_disk_num: 0,
            central_dir_num_entries_this_disk: 1,
            central_dir_num_entries_total: 1,
            central_dir_size: 73,
            central_dir_start_offset: 5218206,
            comment: b""[..].to_vec(),
        }
    );

    assert!(unparsed_5.is_empty());
}

#[test]
fn test_inner_iterion_multi() {
    let data_zip = fs::read("tests/assets/zip/zipped_ab.zip").unwrap();

    let (unparsed_1, parsed_header) =
        LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 16, 18, 17, 30),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"zipped_a.txt"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [48, 106, 212, 90, 41, 106, 212, 90, 53, 50, 50, 131][..].to_vec(),
            )],
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

    let (unparsed_3, parsed_data_desc) =
        DataDescriptor::parse_zip(&unparsed_2).expect("Should be able to parse");

    assert_eq!(
        parsed_data_desc,
        DataDescriptor {
            tag: true,
            crc_32: 1929961249,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u64,
            uncompressed_size: out_pos as u64 % (1024 * 1024 * 1024 * 4),
        }
    );

    println!("Uncompressed {} bytes", out_pos);

    let (unparsed_4, parsed_header) =
        LocalFileHeader::parse(&unparsed_3).expect("Should be able to parse");

    assert_eq!(
        parsed_header,
        LocalFileHeader {
            version_needed: 20,
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod: generate_systime(2018, 4, 16, 18, 17, 58),
            crc_32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            is_zip64: false,
            filename: b"zipped_b.txt"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [72, 106, 212, 90, 70, 106, 212, 90, 53, 50, 50, 131][..].to_vec(),
            )],
        }
    );

    let mut def = deflate::Stream::new();
    let mut out_pos = 0;

    let start = std::time::Instant::now();

    let result = def
        .inner_iter(unparsed_4, |out| {
            out_pos += out.len();
        })
        .expect("Should be able to deflate");

    let end = std::time::Instant::now() - start;

    let unparsed_5 = if let deflate::State::Stop { unparsed_input } = result {
        unparsed_input
    } else {
        panic!("That should have been a full, complete stream!");
    };

    let (unparsed_6, parsed_data_desc) =
        DataDescriptor::parse_zip(&unparsed_5).expect("Should be able to parse");

    assert_eq!(
        parsed_data_desc,
        DataDescriptor {
            tag: true,
            crc_32: 532509087,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u64,
            uncompressed_size: out_pos as u64 % (1024 * 1024 * 1024 * 4),
        }
    );

    println!("Uncompressed {} bytes", out_pos);

    let (unparsed_7, parsed_central_dir_entry) =
        CentralDirHeader::parse(&unparsed_6).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_entry,
        CentralDirHeader {
            version_needed: 20,
            version_made_by: (21, VersionMadeBy::Unix),
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod_time: 37423,
            last_mod_date: 19600,
            crc_32: 1929961249,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u32,
            uncompressed_size: (out_pos as u64 % (1024 * 1024 * 1024 * 4)) as u32,
            disk_no_start: 0,
            int_file_attrib: 0,
            ext_file_attrib: 2175025152,
            rel_offset_loc_header: 0,
            filename: b"zipped_a.txt"[..].to_vec(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [48, 106, 212, 90, 41, 106, 212, 90][..].to_vec(),
            )],
            comment: b""[..].to_vec(),
        }
    );

    let (unparsed_8, parsed_central_dir_entry) =
        CentralDirHeader::parse(&unparsed_7).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_entry,
        CentralDirHeader {
            version_needed: 20,
            version_made_by: (21, VersionMadeBy::Unix),
            encrypted: false,
            deflate_mode: DeflateMode::Normal,
            deferred_sizes: true,
            compression_method: CompressionMethod::Deflated,
            last_mod_time: 37437,
            last_mod_date: 19600,
            crc_32: 532509087,
            compressed_size: (unparsed_1.len() - unparsed_2.len()) as u32,
            uncompressed_size: (out_pos as u64 % (1024 * 1024 * 1024 * 4)) as u32,
            disk_no_start: 0,
            int_file_attrib: 0,
            ext_file_attrib: 2175025152,
            rel_offset_loc_header: 102,
            filename: b"zipped_b.txt"[..].into(),
            extra_fields: vec![(
                HeaderId::InfoZipUnixOriginal,
                [72, 106, 212, 90, 70, 106, 212, 90][..].into(),
            )],
            comment: b""[..].into(),
        }
    );

    let (unparsed_9, parsed_central_dir_end) =
        CentralDirEnd::parse(&unparsed_8).expect("Should be able to parse");

    assert_eq!(
        parsed_central_dir_end,
        CentralDirEnd {
            this_disk_num: 0,
            central_dir_start_disk_num: 0,
            central_dir_num_entries_this_disk: 2,
            central_dir_num_entries_total: 2,
            central_dir_size: 140,
            central_dir_start_offset: 204,
            comment: b""[..].into(),
        }
    );

    assert!(unparsed_9.is_empty());
}

fn print_bytes(input: impl AsRef<[u8]>) {
    println!("{}!", String::from_utf8_lossy(input.as_ref()));
}

#[test]
fn test_multifile() {
    use stream_zipper::State;

    let data_zip = fs::read("tests/assets/zip/rand_data_abc.zip").unwrap();
    let mut file = stream_zipper::start_stream();
    let mut comp_accu = 0;
    let mut uncomp_accu = 0;
    let mut chunks = data_zip.chunks(10 * 1024);

    if let Some(first_chunk) = chunks.next() {
        comp_accu += first_chunk.len();
        let r = file
            .read_internal_iter(&first_chunk, |uncomp| {
                uncomp_accu += uncomp.len();
            })
            .unwrap();
        println!(
            "File name: {}",
            String::from_utf8_lossy(file.name().as_ref().map(|s| &**s).unwrap_or(b""))
        );
        while let Some(chunk) = chunks.next() {
            let mut input = chunk;
            println!(
                "{} KB compressed. {} KB uncompressed.",
                comp_accu / 1024,
                uncomp_accu / 1024
            );
            comp_accu += chunk.len();
            while let State::NextFile {
                unparsed_input,
                next_file,
            } = file
                .read_internal_iter(input, |uncomp| {
                    uncomp_accu += uncomp.len();
                    //print_bytes(uncomp);
                })
                .unwrap()
            {
                println!("Next file: ");
                input = unparsed_input;
                print_bytes(next_file.name().as_ref().unwrap());
                file = next_file;
            }
        }
    }
}
