use deflate;

use std;

use nom;

use gzip::headers::MemberHeader;
use State;

use crate::input_helper::{Input, InputHandler};

pub mod headers;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum GZipError {
    InvalidMemberHeader,
    InvalidDeflateStream,
    InvalidFooter,
}

impl std::error::Error for GZipError {
    fn description(&self) -> &str {
        "zip uncompressing error"
    }
}

impl std::fmt::Display for GZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use self::GZipError::*;
        match self {
            InvalidMemberHeader => write!(f, "invalid member header"),
            InvalidDeflateStream => write!(f, "invalid deflate stream"),
            InvalidFooter => write!(f, "invalid footer"),
        }
    }
}

pub struct GZipFile {
    state: InternalState,
    unparsed: Vec<u8>,
    inflater: deflate::Stream,
}

impl std::fmt::Debug for GZipFile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("GZipFile")
            .field("state", &self.state)
            .field("unparsed", &self.unparsed)
            .finish()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct HeaderParsed {
    header: headers::MemberHeader,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Inflated {
    header: headers::MemberHeader,
    comp_size: usize,
    uncomp_size: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum InternalState {
    Init,
    HeaderParsed(HeaderParsed),
    Inflated(Inflated),
    End(Inflated),
    Eof,
    Sentinel,
    Error,
}

#[derive(Debug)]
enum ParseResult {
    Continue,
    NeedsInput,
    Output,
    Error(GZipError),
    NextFile(GZipFile),
    EndOfFile,
}

impl GZipFile {
    pub fn get_output(&self) -> &[u8] {
        self.inflater.get_output()
    }

    pub fn read<'i, 's>(
        &'s mut self,
        input: &'i [u8],
    ) -> Result<State<'i, 's, GZipFile>, GZipError> {
        let mut ihandler = InputHandler::take_storage(&mut self.unparsed, input);
        let mut unparsed = ihandler.get_unparsed();

        loop {
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
                        ihandler.return_storage(&mut self.unparsed);
                        return Ok(State::NeedsInput);
                    }
                    unparsed = ihandler.get_unparsed();
                }
                ParseResult::Output => {
                    let unparsed_input = unparsed.assert_take_long();
                    return Ok(State::HasOutput {
                        unparsed_input,
                        output: self.inflater.get_output(),
                    });
                }
                ParseResult::NextFile(next_file) => {
                    let unparsed_input = unparsed.assert_take_long();
                    return Ok(State::NextFile {
                        unparsed_input,
                        next_file,
                    });
                }
                ParseResult::EndOfFile => {
                    if self.state == InternalState::Eof {
                        return Ok(State::EndOfFile);
                    } else {
                        return Ok(State::NeedsInputOrEof(start_stream()));
                    }
                }
                ParseResult::Error(err) => return Err(err),
            };
            if unparsed.is_empty() {
                return Ok(State::NeedsInput);
            }
        }
    }

    fn parse_step<'long, 'short>(
        &'short mut self,
        state: InternalState,
        input: Input<'long, 'short>,
    ) -> (usize, InternalState, ParseResult) {
        match state {
            InternalState::Init => GZipFile::parse_header(input),
            InternalState::HeaderParsed(state) => self.inflate(input, state),
            InternalState::Inflated(state) => GZipFile::parse_footer(input, state),
            InternalState::End { .. } => (0, InternalState::Eof, ParseResult::EndOfFile),
            InternalState::Eof => {
                panic!("Don't call read after Eof!");
            }
            InternalState::Sentinel => unreachable!("parse_step is never called with Sentinel"),
            InternalState::Error => panic!("don't call parse_step with Error"),
        }
    }

    fn parse_header<'long, 'short>(
        input: Input<'long, 'short>,
    ) -> (usize, InternalState, ParseResult) {
        match MemberHeader::parse(*input) {
            Ok((unparsed, header)) => {
                let consumed = input.len() - unparsed.len();
                (
                    consumed,
                    InternalState::HeaderParsed(HeaderParsed { header }),
                    ParseResult::Continue,
                )
            }
            Err(nom::Err::Incomplete(_need)) => (0, InternalState::Init, ParseResult::NeedsInput),
            Err(nom::Err::Error(_e)) => (
                0,
                InternalState::Error,
                ParseResult::Error(GZipError::InvalidMemberHeader),
            ),
            Err(nom::Err::Failure(_e)) => (
                0,
                InternalState::Error,
                ParseResult::Error(GZipError::InvalidMemberHeader),
            ),
        }
    }

    fn inflate<'long, 'short>(
        &mut self,
        input: Input<'long, 'short>,
        state: HeaderParsed,
    ) -> (usize, InternalState, ParseResult) {
        match self.inflater.feed_input(*input) {
            Ok(deflate::State::NeedsInput { unparsed_input }) => (
                input.len() - unparsed_input.len(),
                InternalState::HeaderParsed(state),
                ParseResult::Continue,
            ),
            Ok(deflate::State::HasOutput {
                unparsed_input,
                output,
            }) => {
                let consumed_bytes = input.len() - unparsed_input.len();
                (
                    consumed_bytes,
                    InternalState::HeaderParsed(state),
                    ParseResult::Output,
                )
            }
            Ok(deflate::State::Stop { unparsed_input }) => (
                input.len() - unparsed_input.len(),
                InternalState::Inflated(Inflated {
                    header: state.header,
                    comp_size: self.inflater.compressed_size(),
                    uncomp_size: self.inflater.uncompressed_size(),
                }),
                ParseResult::Continue,
            ),
            Err(_) => (
                0,
                InternalState::HeaderParsed(state),
                ParseResult::Error(GZipError::InvalidDeflateStream),
            ),
        }
    }

    fn parse_footer<'long, 'short>(
        input: Input<'long, 'short>,
        state: Inflated,
    ) -> (usize, InternalState, ParseResult) {
        match headers::parse_footer(*input) {
            Ok((mut unparsed, footer)) => {
                if unparsed.is_empty() {
                    let consumed = input.len() - unparsed.len();
                    return (consumed, InternalState::End(state), ParseResult::EndOfFile);
                }

                let res = match peek_stream(unparsed) {
                    Ok((unparsed_input, next_file)) => {
                        unparsed = unparsed_input;
                        ParseResult::NextFile(next_file)
                    }
                    Err(err) => {
                        return (0, InternalState::Inflated(state), ParseResult::Error(err));
                    }
                };

                let consumed = input.len() - unparsed.len();
                (consumed, InternalState::End(state), res)
            }
            Err(nom::Err::Incomplete(_need)) => {
                (0, InternalState::Inflated(state), ParseResult::NeedsInput)
            }
            Err(nom::Err::Error(_e)) => (
                0,
                InternalState::Inflated(state),
                ParseResult::Error(GZipError::InvalidFooter),
            ),
            Err(nom::Err::Failure(_e)) => (
                0,
                InternalState::Inflated(state),
                ParseResult::Error(GZipError::InvalidFooter),
            ),
        }
    }

    pub fn filename(&self) -> Option<&[u8]> {
        match &self.state {
            InternalState::HeaderParsed(HeaderParsed { header }) => header,
            InternalState::Inflated(Inflated { header, .. }) => header,
            InternalState::End(Inflated { header, .. }) => header,
            _ => return None,
        }
        .filename
        .as_ref()
        .map(|f| f.as_slice())
    }

    pub fn read_with<'i>(
        &mut self,
        mut input: &'i [u8],
        mut callback: impl FnMut(&[u8]),
    ) -> Result<crate::State<'i, 'i, GZipFile>, GZipError> {
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
}

/// Stats a gzip stream.
pub fn start_stream() -> GZipFile {
    GZipFile {
        state: InternalState::Init,
        unparsed: Vec::new(),
        inflater: deflate::Stream::new(),
    }
}

pub fn peek_stream(input: &[u8]) -> Result<(&[u8], GZipFile), GZipError> {
    match MemberHeader::parse(input) {
        Ok((unparsed, header)) => Ok((
            unparsed,
            GZipFile {
                state: InternalState::Init,
                unparsed: Vec::new(),
                inflater: deflate::Stream::new(),
            },
        )),
        Err(nom::Err::Incomplete(_need)) => Ok((
            &b""[..],
            GZipFile {
                state: InternalState::Init,
                unparsed: Vec::new(),
                inflater: deflate::Stream::new(),
            },
        )),
        Err(nom::Err::Error(_e)) => Err(GZipError::InvalidMemberHeader),
        Err(nom::Err::Failure(_e)) => Err(GZipError::InvalidMemberHeader),
    }
}
