use std::time::SystemTime;

use nom::bits::{bits, streaming::take as take_bits};
use nom::bytes::streaming::tag as btag;
use nom::number::complete::le_u16 as complete_le_u16;
use nom::number::streaming::{le_u16, le_u32, le_u64, le_u8};
use nom::sequence::{pair, tuple};
use nom::IResult;
use nom::{call, do_parse, length_value, many0, opt, tag, take, value};

use crate::utils::{fail, flat_map, map_err, parse_bit_to_bool, NomErrorExt};

use super::datetime::parse_msdos_datetime;
use super::ZipError;

pub const LOCAL_FILE_HEADER_TAG: &[u8] = b"\x50\x4b\x03\x04";
pub const DATA_DESCRIPTOR_TAG: &[u8] = b"\x50\x4b\x07\x08";
pub const CENTRAL_DIR_HEADER_TAG: &[u8] = b"\x50\x4b\x01\x02";
pub const CENTRAL_DIR_END_TAG: &[u8] = b"\x50\x4b\x05\x06";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalFileHeader {
    pub version_needed: u16,
    pub encrypted: bool,
    pub deflate_mode: DeflateMode,
    pub deferred_sizes: bool,
    pub compression_method: CompressionMethod,
    pub last_mod: SystemTime,
    pub crc_32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub filename: Vec<u8>,
    pub is_zip64: bool,
    pub extra_fields: Vec<(HeaderId, Vec<u8>)>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum VersionMadeBy {
    MsDos,
    Amiga,
    OpenVms,
    Unix,
    VmCms,
    AtariSt,
    Os2Hpfs,
    Macintosh,
    ZSystem,
    Cpm,
    WindowsNtfs,
    Mvs,
    Vse,
    AcornRisc,
    VFat,
    AlternateMvs,
    BeOs,
    Tandem,
    Os400,
    OsXDarwin,
}

fn parse_version_made_by(input: &[u8]) -> IResult<&[u8], (u8, VersionMadeBy), ZipError> {
    let ver = map_err(pair(le_u8, le_u8), |()| ZipError::InvalidVersionMadeBy);

    use self::VersionMadeBy::*;
    flat_map(ver, |(zip_ver, tag)| {
        Ok((
            zip_ver,
            match tag {
                00 => MsDos,
                01 => Amiga,
                02 => OpenVms,
                03 => Unix,
                04 => VmCms,
                05 => AtariSt,
                06 => Os2Hpfs,
                07 => Macintosh,
                08 => ZSystem,
                09 => Cpm,
                10 => WindowsNtfs,
                11 => Mvs,
                12 => Vse,
                13 => AcornRisc,
                14 => VFat,
                15 => AlternateMvs,
                16 => BeOs,
                17 => Tandem,
                18 => Os400,
                19 => OsXDarwin,
                _ => return fail(ZipError::InvalidVersionMadeBy),
            },
        ))
    })(input)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompressionMethod {
    Stored,
    Shrunk,
    ReducedX1,
    ReducedX2,
    ReducedX3,
    ReducedX4,
    Imploded,
    ReservedTokenized,
    Deflated,
    EnhancedDeflated,
    PkWareDCLImploded,
    Reserved2,
    Bzip2,
    Reserved3,
    Lzma,
    Reserved4,
    Reserved5,
    Reserved6,
    IbmTerse,
    IbmLz77,
    WavPack,
    PpmdVer1Rev1,
}

fn parse_compression_method(input: &[u8]) -> IResult<&[u8], CompressionMethod, ZipError> {
    use self::CompressionMethod::*;

    let (unparsed, tag) =
        le_u16::<()>(input).map_nom_err(|_| ZipError::InvalidCompressionMethod)?;

    Ok((
        unparsed,
        match tag {
            0 => Stored,
            1 => Shrunk,
            2 => ReducedX1,
            3 => ReducedX2,
            4 => ReducedX3,
            5 => ReducedX4,
            6 => Imploded,
            7 => ReservedTokenized,
            8 => Deflated,
            9 => EnhancedDeflated,
            10 => PkWareDCLImploded,
            11 => Reserved2,
            12 => Bzip2,
            13 => Reserved3,
            14 => Lzma,
            15 => Reserved4,
            16 => Reserved5,
            17 => Reserved6,
            18 => IbmTerse,
            19 => IbmLz77,
            97 => WavPack,
            98 => PpmdVer1Rev1,
            _ => return fail(ZipError::InvalidCompressionMethod),
        },
    ))
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HeaderId {
    Zip64Extended,
    AvInfo,
    ReservedExtLangEncData,
    Os2,
    Ntfs,
    OpenVms,
    Unix,
    ReservedFileStreamForkDesc,
    PatchDesc,
    PKCS7StoreForX509Certs,
    X509CertIDAndSignatureForFile,
    X509CertIdForCentralDir,
    StrongEncHeader,
    RecordManagementControls,
    PKCS7EncRecipientCertificateList,
    IBMS390As400AttributesUncompressed,
    ReservedIBMS390As400AttributesCompressed,
    Poszip4690,
    Macintosh,
    ZipItMacintosh,
    ZipItMacintosh135Plus,
    ZipItMacintosh135Plus2,
    InfoZipMacintosh,
    AcornSparkFs,
    WindowsNtSecurityDescriptor,
    VmCms,
    Mvs,
    FwkcsMd5,
    Os2Acl,
    InfoZipOpenVms,
    XceedOrigLoc,
    AosVs,
    ExtendedTimestamp,
    XceedUnicodeXtraField,
    InfoZipUnixOriginal,
    InfoZipUnicodeComment,
    BeOsBeBox,
    AsiUnix,
    InfoZipUnixNew,
    MicrosoftOpenPackagingGrowthHint,
    SmsQDos,
}

fn parse_header_id(input: &[u8]) -> IResult<&[u8], HeaderId, ZipError> {
    use self::HeaderId::*;

    // We parse using complete::le_u16 instead of streaming,
    // because we don't want to throw Incomplete during
    // many0 parsing, which wouldn't make any sense
    let (unparsed, tag) =
        complete_le_u16::<()>(input).map_nom_err(|_| ZipError::InvalidHeaderId)?;

    Ok((
        unparsed,
        match tag {
            0x0001 => Zip64Extended,
            0x0007 => AvInfo,
            0x0008 => ReservedExtLangEncData,
            0x0009 => Os2,
            0x000a => Ntfs,
            0x000c => OpenVms,
            0x000d => Unix,
            0x000e => ReservedFileStreamForkDesc,
            0x000f => PatchDesc,
            0x0014 => PKCS7StoreForX509Certs,
            0x0015 => X509CertIDAndSignatureForFile,
            0x0016 => X509CertIdForCentralDir,
            0x0017 => StrongEncHeader,
            0x0018 => RecordManagementControls,
            0x0019 => PKCS7EncRecipientCertificateList,
            0x0065 => IBMS390As400AttributesUncompressed,
            0x0066 => ReservedIBMS390As400AttributesCompressed,
            0x4690 => Poszip4690,
            0x07c8 => Macintosh,
            0x2605 => ZipItMacintosh,
            0x2705 => ZipItMacintosh135Plus,
            0x2805 => ZipItMacintosh135Plus2,
            0x334d => InfoZipMacintosh,
            0x4341 => AcornSparkFs,
            0x4453 => WindowsNtSecurityDescriptor,
            0x4704 => VmCms,
            0x470f => Mvs,
            0x4b46 => FwkcsMd5,
            0x4c41 => Os2Acl,
            0x4d49 => InfoZipOpenVms,
            0x4f4c => XceedOrigLoc,
            0x5356 => AosVs,
            0x5455 => ExtendedTimestamp,
            0x554e => XceedUnicodeXtraField,
            0x5855 => InfoZipUnixOriginal,
            0x6375 => InfoZipUnicodeComment,
            0x6542 => BeOsBeBox,
            0x756e => AsiUnix,
            0x7855 => InfoZipUnixNew,
            0xa220 => MicrosoftOpenPackagingGrowthHint,
            0xfd4a => SmsQDos,
            _ => return fail(ZipError::InvalidHeaderId),
        },
    ))
}

fn parse_one_extra_field(i: &[u8]) -> IResult<&[u8], (HeaderId, Vec<u8>), ZipError> {
    do_parse!(
        i,
        tag: parse_header_id
            >> len: complete_le_u16
            >> contents: take!(len)
            >> ((tag, contents.to_vec()))
    )
}

fn parse_extra_fields(
    input: &[u8],
    len: u16,
) -> IResult<&[u8], Vec<(HeaderId, Vec<u8>)>, ZipError> {
    length_value!(input, value!(len), many0!(parse_one_extra_field))
}

impl LocalFileHeader {
    pub fn parse(i: &[u8]) -> IResult<&[u8], LocalFileHeader, ZipError> {
        let (i, _) =
            btag(LOCAL_FILE_HEADER_TAG)(i).map_nom_err(|_: ()| ZipError::NotLocalFileHeader)?;
        let (
            i,
            (
                version_needed,
                bit_flags,
                compression_method,
                last_mod,
                crc_32,
                compressed_size,
                uncompressed_size,
                fname_len,
                extra_field_len,
            ),
        ) = tuple::<&[u8], _, ZipError, _>((
            le_u16,
            parse_bitflags,
            parse_compression_method,
            parse_msdos_datetime,
            le_u32,
            le_u32,
            le_u32,
            le_u16,
            le_u16,
        ))(i)
        .map_nom_err(|e| {
            if let ZipError::NomError(_) = e {
                ZipError::InvalidLocalFileHeader
            } else {
                e
            }
        })?;

        let (i, filename) = nom::bytes::streaming::take::<_, _, ZipError>(fname_len)(i)
            .map_nom_err(|_| ZipError::InvalidLocalFileHeader)?;
        let (i, extra_fields) = parse_extra_fields(i, extra_field_len)
            .map_nom_err(|e| ZipError::InvalidLocalFileHeader)?;
        Ok((
            i,
            LocalFileHeader {
                version_needed,
                encrypted: bit_flags.0,
                deflate_mode: bit_flags.1,
                deferred_sizes: bit_flags.2,
                compression_method,
                last_mod,
                crc_32,
                is_zip64: (compressed_size == std::u32::MAX || uncompressed_size == std::u32::MAX),
                compressed_size,
                uncompressed_size,
                filename: filename.to_vec(),
                extra_fields: extra_fields.to_vec(),
            },
        ))
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataDescriptor {
    pub tag: bool,
    pub crc_32: u32,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
}

impl DataDescriptor {
    pub fn parse_zip(i: &[u8]) -> IResult<&[u8], DataDescriptor, ZipError> {
        do_parse!(
            i,
            tag: opt!(btag(DATA_DESCRIPTOR_TAG))
                >> crc_32: le_u32
                >> compressed_size: le_u32
                >> uncompressed_size: le_u32
                >> (DataDescriptor {
                    tag: tag.is_some(),
                    crc_32,
                    uncompressed_size: uncompressed_size as u64,
                    compressed_size: compressed_size as u64
                })
        )
    }

    pub fn parse_zip64(i: &[u8]) -> IResult<&[u8], DataDescriptor, ZipError> {
        do_parse!(
            i,
            tag: opt!(btag(DATA_DESCRIPTOR_TAG))
                >> crc_32: le_u32
                >> compressed_size: le_u64
                >> uncompressed_size: le_u64
                >> (DataDescriptor {
                    tag: tag.is_some(),
                    crc_32,
                    uncompressed_size,
                    compressed_size
                })
        )
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DeflateMode {
    Normal,
    Max,
    Fast,
    SuperFast,
}

fn parse_deflate_mode(input: (&[u8], usize)) -> IResult<(&[u8], usize), DeflateMode, ZipError> {
    let (unparsed, bits) = take_bits(2_usize)(input)?;

    Ok((
        unparsed,
        match bits {
            0 => DeflateMode::Normal,
            1 => DeflateMode::Max,
            2 => DeflateMode::Fast,
            3 => DeflateMode::SuperFast,
            _ => unreachable!("Only these four bit patterns are possible with two bits."),
        },
    ))
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CentralDirHeader {
    pub version_made_by: (u8, VersionMadeBy),
    pub version_needed: u16,
    pub encrypted: bool,
    pub deflate_mode: DeflateMode,
    pub deferred_sizes: bool,
    pub compression_method: CompressionMethod,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc_32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub disk_no_start: u16,
    pub int_file_attrib: u16,
    pub ext_file_attrib: u32,
    pub rel_offset_loc_header: u32,
    pub filename: Vec<u8>,
    pub extra_fields: Vec<(HeaderId, Vec<u8>)>,
    pub comment: Vec<u8>,
}

pub fn parse_bitflags(input: &[u8]) -> IResult<&[u8], (bool, DeflateMode, bool), ZipError> {
    let (input, (_pad1, deferred_sizes, deflate_mode, encrypted, _pad2)) = bits(tuple((
        take_bits(4_usize),
        parse_bit_to_bool,
        parse_deflate_mode,
        parse_bit_to_bool,
        take_bits(4_usize),
    )))(input)?;
    let _: u8 = _pad1;
    let _: u8 = _pad2;
    Ok((input, (encrypted, deflate_mode, deferred_sizes)))
}

impl CentralDirHeader {
    pub fn parse(i: &[u8]) -> IResult<&[u8], CentralDirHeader, ZipError> {
        let (i, _) =
            btag(CENTRAL_DIR_HEADER_TAG)(i).map_nom_err(|_: ()| ZipError::NotCentralDirHeader)?;
        do_parse!(
            i,
            version_made_by: parse_version_made_by
                >> version_needed: le_u16
                >> bit_flags: parse_bitflags
                >> compression_method: parse_compression_method
                >> last_mod_time: le_u16
                >> last_mod_date: le_u16
                >> crc_32: le_u32
                >> compressed_size: le_u32
                >> uncompressed_size: le_u32
                >> fname_len: le_u16
                >> extra_field_len: le_u16
                >> fcomment_len: le_u16
                >> disk_no_start: le_u16
                >> int_file_attrib: le_u16
                >> ext_file_attrib: le_u32
                >> rel_offset_loc_header: le_u32
                >> filename: take!(fname_len)
                >> extra_fields: call!(parse_extra_fields, extra_field_len)
                >> comment: take!(fcomment_len)
                >> (CentralDirHeader {
                    version_made_by,
                    version_needed,
                    encrypted: bit_flags.0,
                    deflate_mode: bit_flags.1,
                    deferred_sizes: bit_flags.2,
                    compression_method,
                    last_mod_time,
                    last_mod_date,
                    crc_32,
                    compressed_size,
                    uncompressed_size,
                    disk_no_start,
                    int_file_attrib,
                    ext_file_attrib,
                    rel_offset_loc_header,
                    filename: filename.to_vec(),
                    extra_fields,
                    comment: comment.to_vec(),
                })
        )
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CentralDirEnd {
    pub this_disk_num: u16,
    pub central_dir_start_disk_num: u16,
    pub central_dir_num_entries_this_disk: u16,
    pub central_dir_num_entries_total: u16,
    pub central_dir_size: u32,
    pub central_dir_start_offset: u32,
    pub comment: Vec<u8>,
}

impl CentralDirEnd {
    pub fn parse(i: &[u8]) -> IResult<&[u8], CentralDirEnd> {
        do_parse!(
            i,
            tag!(CENTRAL_DIR_END_TAG)
                >> this_disk_num: le_u16
                >> central_dir_start_disk_num: le_u16
                >> central_dir_num_entries_this_disk: le_u16
                >> central_dir_num_entries_total: le_u16
                >> central_dir_size: le_u32
                >> central_dir_start_offset: le_u32
                >> comment_len: le_u16
                >> comment: take!(comment_len)
                >> (CentralDirEnd {
                    this_disk_num,
                    central_dir_start_disk_num,
                    central_dir_num_entries_this_disk,
                    central_dir_num_entries_total,
                    central_dir_size,
                    central_dir_start_offset,
                    comment: comment.to_vec(),
                })
        )
    }
}
