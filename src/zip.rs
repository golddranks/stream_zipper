use std::ops::Not;

use nom;
use nom::error::ErrorKind;

use crate::deflate;
use crate::input_helper::{Input, InputHandler};
use crate::{CompressedStream, ReadHeadersResult, State};

pub struct ZipFile {
    state: InternalState,
    inflater: deflate::Stream,
    unparsed: Vec<u8>,
}

impl std::fmt::Debug for ZipFile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("ZipFile")
            .field("state", &self.state)
            .field("unparsed", &self.unparsed)
            .finish()
    }
}

impl CompressedStream for ZipFile {
    fn feed_input(&mut self, input: &[u8]) -> State<Self> {
        unimplemented!();
    }
}

pub mod datetime;
pub mod headers;

use self::headers::{CentralDirHeader, DataDescriptor, LocalFileHeader};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ZipError {
    InvalidDateOrTime,
    InvalidVersionMadeBy,
    InvalidCompressionMethod,
    InvalidHeaderId,
    NotLocalFileHeader,
    InvalidLocalFileHeader,
    InvalidDeflateStream,
    InvalidDataDescriptor,
    NotCentralDirHeader,
    InvalidCentralDirHeader,
    NomError(ErrorKind),
    OtherError,
}

impl nom::error::ParseError<&[u8]> for ZipError {
    fn from_error_kind(_input: &[u8], kind: ErrorKind) -> Self {
        ZipError::NomError(kind)
    }

    fn append(_: &[u8], _: ErrorKind, other: Self) -> Self {
        other
    }
}

impl nom::error::ParseError<(&[u8], usize)> for ZipError {
    fn from_error_kind(_input: (&[u8], usize), kind: ErrorKind) -> Self {
        ZipError::NomError(kind)
    }

    fn append(_input: (&[u8], usize), _kind: ErrorKind, other: Self) -> Self {
        other
    }
}

impl nom::ErrorConvert<ZipError> for ZipError {
    fn convert(self) -> ZipError {
        self
    }
}

impl From<()> for ZipError {
    fn from(_: ()) -> ZipError {
        ZipError::OtherError
    }
}

impl std::error::Error for ZipError {
    fn description(&self) -> &str {
        "zip uncompressing error"
    }
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "zip uncompressing error: {:?}", self)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct HeaderParsed {
    header: LocalFileHeader,
}
#[derive(Debug, Clone, Eq, PartialEq)]
struct Inflated {
    header: LocalFileHeader,
    comp_size: usize,
    uncomp_size: usize,
}
#[derive(Debug, Clone, Eq, PartialEq)]
struct DescriptorParsed {
    header: LocalFileHeader,
    comp_size: usize,
    uncomp_size: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum InternalState {
    Init,
    HeaderParsed(HeaderParsed),
    Inflated(Inflated),
    DescriptorParsed(DescriptorParsed),
    End(DescriptorParsed),
    Sentinel,
    Error,
}

#[derive(Debug)]
enum ParseResult {
    Continue,
    NeedsInput,
    Output,
    Error(ZipError),
    NextFile(ZipFile),
    EndOfFile,
}

impl ZipFile {
    pub fn get_output(&self) -> &[u8] {
        self.inflater.get_output()
    }

    pub fn read_headers<'i>(&mut self, input: &'i [u8]) -> Result<ReadHeadersResult<'i>, ZipError> {
        if let InternalState::Init = self.state {
        } else {
            return Ok(ReadHeadersResult::Done { unparsed: input });
        }

        let mut ihandler = InputHandler::take_storage(&mut self.unparsed, input);
        let mut unparsed = ihandler.get_unparsed();
        let res = loop {
            let (bytes_consumed, new_state, res) = ZipFile::parse_header(unparsed);
            unparsed = ihandler.consumed(bytes_consumed);
            self.state = new_state;
            match res {
                ParseResult::Continue => {
                    break Ok(ReadHeadersResult::Done {
                        unparsed: unparsed.assert_take_long(),
                    });
                }
                ParseResult::NeedsInput => {
                    let ext_len = ihandler.extend_input();
                    if ext_len == 0 {
                        break Ok(ReadHeadersResult::NeedsInput);
                    }
                    unparsed = ihandler.get_unparsed();
                }
                ParseResult::Error(err) => break Err(err),
                _ => {
                    unreachable!();
                }
            };
        };
        ihandler.return_storage(&mut self.unparsed);
        res
    }

    pub fn read<'i, 's>(&'s mut self, input: &'i [u8]) -> Result<State<'i, 's, ZipFile>, ZipError>
    where
        'i: 's,
    {
        let mut ihandler = InputHandler::take_storage(&mut self.unparsed, input);
        let mut unparsed = ihandler.get_unparsed();
        let res = loop {
            let mut state = InternalState::Sentinel;
            std::mem::swap(&mut self.state, &mut state);
            let (bytes_consumed, new_state, res) = self.parse_step(state, unparsed);
            unparsed = ihandler.consumed(bytes_consumed);
            self.state = new_state;
            match res {
                ParseResult::Continue => (),
                ParseResult::NeedsInput => {
                    let extended_len = ihandler.extend_input();
                    // Nothing in input left to extend, so we need the user to provide more
                    if extended_len == 0 {
                        break Ok(State::NeedsInput);
                    }
                    unparsed = ihandler.get_unparsed();
                }
                ParseResult::Output => {
                    let unparsed_input = unparsed.assert_take_long();
                    break Ok(State::HasOutput {
                        unparsed_input,
                        output: self.inflater.get_output(),
                    });
                }
                ParseResult::NextFile(next_file) => {
                    let unparsed_input = unparsed.assert_take_long();
                    break Ok(State::NextFile {
                        unparsed_input,
                        next_file,
                    });
                }
                ParseResult::EndOfFile => {
                    break Ok(State::EndOfFile);
                }
                ParseResult::Error(err) => break Err(err),
            };
            if unparsed.is_empty() {
                break Ok(State::NeedsInput);
            }
        };
        ihandler.return_storage(&mut self.unparsed);
        res
    }

    pub fn read_with<'i>(
        &mut self,
        mut input: &'i [u8],
        mut callback: impl FnMut(&[u8]),
    ) -> Result<crate::State<'i, 'i, ZipFile>, ZipError> {
        loop {
            let state = self.read(input)?;
            if let State::HasOutput {
                unparsed_input,
                output,
            } = state
            {
                input = unparsed_input;
                callback(output);
            } else {
                return Ok(state.assert_no_output());
            }
        }
    }

    fn parse_step<'long, 'short>(
        &'short mut self,
        state: InternalState,
        input: Input<'long, 'short>,
    ) -> (usize, InternalState, ParseResult) {
        match state {
            InternalState::Init => ZipFile::parse_header(input),
            InternalState::HeaderParsed(state) => self.inflate(input, state),
            InternalState::Inflated(state) => ZipFile::parse_descriptor(input, state),
            InternalState::DescriptorParsed(state) => ZipFile::end(input, state),
            end_state @ InternalState::End { .. } => (0, end_state, ParseResult::EndOfFile),
            InternalState::Sentinel => unreachable!("parse_step is never called with Sentinel"),
            InternalState::Error => panic!("Don't call read with Error"),
        }
    }

    fn parse_header<'long, 'short>(
        input: Input<'long, 'short>,
    ) -> (usize, InternalState, ParseResult) {
        match LocalFileHeader::parse(*input) {
            Ok((unparsed, header)) => {
                let bytes_parsed = input.len() - unparsed.len();
                let inflater = deflate::Stream::new();
                (
                    bytes_parsed,
                    InternalState::HeaderParsed(HeaderParsed { header }),
                    ParseResult::Continue,
                )
            }
            Err(nom::Err::Incomplete(_need)) => (0, InternalState::Init, ParseResult::NeedsInput),
            Err(nom::Err::Error(_e)) => (
                0,
                InternalState::Error,
                ParseResult::Error(ZipError::InvalidLocalFileHeader),
            ),
            Err(nom::Err::Failure(_e)) => (
                0,
                InternalState::Error,
                ParseResult::Error(ZipError::InvalidLocalFileHeader),
            ),
        }
    }

    fn detect_empty_stream(
        input: &[u8],
        state: HeaderParsed,
    ) -> Result<HeaderParsed, (usize, InternalState, ParseResult)> {
        if state.header.compressed_size == 0
            && state.header.uncompressed_size == 0
            && state.header.deferred_sizes.not()
        {
            if input.len() < 4 {
                return Err((
                    0,
                    InternalState::HeaderParsed(state),
                    ParseResult::NeedsInput,
                ));
            }
            // The next file header starts directly so the
            // Deflate stream was 0 bytes long
            if &input[..4] == headers::LOCAL_FILE_HEADER_TAG {
                return Err((
                    0,
                    InternalState::Inflated(Inflated {
                        header: state.header,
                        uncomp_size: 0,
                        comp_size: 0,
                    }),
                    ParseResult::Continue,
                ));
            }
        }
        Ok(state)
    }

    fn inflate<'l, 's>(
        &'s mut self,
        input: Input<'l, 's>,
        state: HeaderParsed,
    ) -> (usize, InternalState, ParseResult)
    where
        'l: 's,
    {
        let state = match ZipFile::detect_empty_stream(*input, state) {
            Ok(state) => state,
            Err(result) => return result,
        };

        let HeaderParsed { header } = state;

        match self.inflater.feed_input(*input) {
            Ok(deflate::State::NeedsInput { unparsed_input }) => (
                input.len() - unparsed_input.len(),
                InternalState::HeaderParsed(HeaderParsed { header }),
                ParseResult::Continue,
            ),
            Ok(deflate::State::HasOutput {
                unparsed_input,
                output,
            }) => {
                let consumed_bytes = input.len() - unparsed_input.len();
                (
                    consumed_bytes,
                    InternalState::HeaderParsed(HeaderParsed { header }),
                    ParseResult::Output,
                )
            }
            Ok(deflate::State::Stop { unparsed_input }) => (
                input.len() - unparsed_input.len(),
                InternalState::Inflated(Inflated {
                    header,
                    comp_size: self.inflater.compressed_size(),
                    uncomp_size: self.inflater.uncompressed_size(),
                }),
                ParseResult::Continue,
            ),
            Err(_) => (
                0,
                InternalState::HeaderParsed(HeaderParsed { header }),
                ParseResult::Error(ZipError::InvalidDeflateStream),
            ),
        }
    }

    fn parse_descriptor(
        input: Input<'_, '_>,
        state: Inflated,
    ) -> (usize, InternalState, ParseResult) {
        let desc_res = if state.header.is_zip64 {
            // Parsing these will always succeed, but if the data descriptor doesn't exist, the values are garbage
            DataDescriptor::parse_zip64(*input)
        } else {
            DataDescriptor::parse_zip(*input)
        };

        match desc_res {
            Ok((unparsed, desc)) => {
                let desc_must_exist = desc.tag || state.header.deferred_sizes;

                let actual_uncomp_size;
                let actual_comp_size;

                if state.header.is_zip64 {
                    // The sizes are actual sizes, not moduluses
                    actual_uncomp_size = state.uncomp_size as u64;
                    actual_comp_size = state.comp_size as u64;
                } else {
                    // Some archivers store the file size as a modulus of 2^32 if it's over 4 GiB
                    actual_uncomp_size = (state.uncomp_size as u64) % (std::u32::MAX as u64 + 1);
                    actual_comp_size = (state.comp_size as u64) % (std::u32::MAX as u64 + 1);
                }

                let data_matches = actual_uncomp_size == desc.uncompressed_size
                    && actual_comp_size as u64 == desc.compressed_size;

                let dparsed = DescriptorParsed {
                    header: state.header,
                    comp_size: state.comp_size,
                    uncomp_size: state.uncomp_size,
                };
                if data_matches {
                    return (
                        input.len() - unparsed.len(),
                        InternalState::DescriptorParsed(dparsed),
                        ParseResult::Continue,
                    );
                } else {
                    if desc_must_exist {
                        return (
                            0,
                            InternalState::Error,
                            ParseResult::Error(ZipError::InvalidDataDescriptor),
                        );
                    } else {
                        // Data was garbage, but the descriptor wasn't required to exist so it's good.
                        return (
                            0,
                            InternalState::DescriptorParsed(dparsed),
                            ParseResult::Continue,
                        );
                    }
                }
            }
            Err(nom::Err::Incomplete(_)) => {
                return (0, InternalState::Inflated(state), ParseResult::NeedsInput);
            }
            Err(_) => {
                unreachable!(
                    "The data descriptor parsing always succeeds so no other errors are possible!"
                );
            } // No match - the data descriptor doesn't exist
        }
    }

    fn end<'long, 'short>(
        input: Input<'long, 'short>,
        state: DescriptorParsed,
    ) -> (usize, InternalState, ParseResult) {
        match peek_stream(*input) {
            Ok((unparsed, next_file)) => {
                let bytes_parsed = input.len() - unparsed.len();
                return (
                    bytes_parsed,
                    InternalState::End(state),
                    ParseResult::NextFile(next_file),
                );
            }
            Err(ZipError::NotLocalFileHeader) => (),
            Err(e) => return (0, InternalState::Error, ParseResult::Error(e)),
        };

        match CentralDirHeader::parse(*input) {
            Ok(_header) => {
                return (0, InternalState::End(state), ParseResult::Continue);
            }
            Err(nom::Err::Incomplete(_)) => {
                return (
                    0,
                    InternalState::DescriptorParsed(state),
                    ParseResult::NeedsInput,
                )
            }
            Err(_) => {
                return (
                    0,
                    InternalState::Error,
                    ParseResult::Error(ZipError::InvalidCentralDirHeader),
                )
            }
        }
    }

    pub fn filename(&self) -> Option<&[u8]> {
        match &self.state {
            InternalState::Init => None,
            InternalState::HeaderParsed(state) => Some(&state.header.filename),
            InternalState::Inflated(state) => Some(&state.header.filename),
            InternalState::DescriptorParsed(state) => Some(&state.header.filename),
            InternalState::End(state) => Some(&state.header.filename),
            InternalState::Sentinel => unreachable!("filename is never called with this"),
            InternalState::Error => panic!("this shouldn't be called after an error"),
        }
        .map(|n| &**n)
    }
}

pub fn start_stream() -> ZipFile {
    ZipFile {
        state: InternalState::Init,
        unparsed: Vec::new(),
        inflater: deflate::Stream::new(),
    }
}

pub fn peek_stream(input: &[u8]) -> Result<(&[u8], ZipFile), ZipError> {
    match LocalFileHeader::parse(input) {
        Ok((unparsed, header)) => Ok((
            unparsed,
            ZipFile {
                state: InternalState::HeaderParsed(HeaderParsed { header }),
                unparsed: Vec::new(),
                inflater: deflate::Stream::new(),
            },
        )),
        Err(nom::Err::Incomplete(_need)) => Ok((
            &[],
            ZipFile {
                state: InternalState::Init,
                unparsed: input.to_vec(),
                inflater: deflate::Stream::new(),
            },
        )),
        Err(nom::Err::Error(ZipError::NotLocalFileHeader)) => Err(ZipError::NotLocalFileHeader),
        Err(nom::Err::Error(_e)) => Err(ZipError::InvalidLocalFileHeader),
        Err(nom::Err::Failure(_e)) => Err(ZipError::InvalidLocalFileHeader),
    }
}
