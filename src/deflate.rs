use std::io::Cursor;

use miniz_oxide::inflate::core::DecompressorOxide;
use miniz_oxide::inflate::TINFLStatus;

struct InnerState {
    output: Vec<u8>,
    out_pos: usize,
    last_out_pos: usize,
    decomp: DecompressorOxide,
    flags: u32,
    uncomp_size: usize,
    comp_size: usize,
    had_output: bool,
}

impl InnerState {
    fn get_output(&self) -> &[u8] {
        if self.last_out_pos < self.out_pos {
            &self.output[self.last_out_pos..self.out_pos]
        } else {
            &self.output[self.last_out_pos..]
        }
    }
}

/*
pub struct ChunkIter<'a> {
    input: &'a [u8],
    stream_continues: bool,
    state: InnerState,
}
*/

pub struct Stream {
    state: InnerState,
}

#[derive(Eq, Debug, Clone, Copy, PartialEq)]
pub enum State<'i, 'o> {
    HasOutput {
        unparsed_input: &'i [u8],
        output: &'o [u8],
    },
    NeedsInput {
        unparsed_input: &'i [u8],
    },
    Stop {
        unparsed_input: &'i [u8],
    },
}

fn consume_input<'i, 'o>(
    input: &'i [u8],
    mut state: &'o mut InnerState,
) -> Result<State<'i, 'o>, TINFLStatus> {
    use miniz_oxide::inflate::core::decompress;
    use miniz_oxide::inflate::TINFLStatus::*;

    if state.had_output {
        state.had_output = false;
        state.last_out_pos = state.out_pos;
    }

    let (status, in_consumed, out_consumed) = {
        // Wrap the whole output slice so we know we have enough of the
        // decompressed data for matches.
        let mut c = Cursor::new(state.output.as_mut_slice());
        c.set_position(state.out_pos as u64);
        decompress(&mut state.decomp, &input[..], &mut c, state.flags)
    };

    state.comp_size += in_consumed;
    state.uncomp_size += out_consumed;
    state.out_pos += out_consumed;
    let unparsed_input = &input[in_consumed..];

    debug_assert!(state.out_pos <= state.output.len());
    if state.out_pos == state.output.len() {
        state.out_pos = 0;
    }

    match status {
        Done => {
            if out_consumed == 0 {
                return Ok(State::Stop { unparsed_input });
            } else {
                state.had_output = true;
                let output = state.get_output();
                return Ok(State::HasOutput {
                    unparsed_input,
                    output,
                });
            }
        }
        NeedsMoreInput => {
            //if out_consumed == 0 {
            return Ok(State::NeedsInput { unparsed_input });
            /*    } else {
                return Ok(State::HasOutput {
                    unparsed_input,
                    output,
                });
            }*/
        }
        HasMoreOutput => {
            state.had_output = true;
            let output = state.get_output();
            return Ok(State::HasOutput {
                unparsed_input,
                output,
            });
        }
        _ => return Err(status),
    }
}

pub fn start_deflate_stream() -> Stream {
    Stream::new()
}

impl Stream {
    pub fn new() -> Stream {
        Self::with(0, 0)
    }

    pub fn with(size: usize, flags: u32) -> Self {
        use miniz_oxide::inflate::core::inflate_flags;
        use miniz_oxide::inflate::core::TINFL_LZ_DICT_SIZE;
        use std::cmp::max;

        let flags = flags | inflate_flags::TINFL_FLAG_HAS_MORE_INPUT;

        let size = max(TINFL_LZ_DICT_SIZE, size);

        let mut output = Vec::with_capacity(size);
        output.resize(size, 0);

        let mut decomp = DecompressorOxide::new();
        decomp.init();

        Self {
            state: InnerState {
                decomp,
                output,
                out_pos: 0,
                last_out_pos: 0,
                flags,
                uncomp_size: 0,
                comp_size: 0,
                had_output: false,
            },
        }
    }

    pub fn feed_input<'i, 'o>(&'o mut self, input: &'i [u8]) -> Result<State<'i, 'o>, TINFLStatus> {
        consume_input(input, &mut self.state)
    }

    pub fn get_output(&self) -> &[u8] {
        self.state.get_output()
    }

    pub fn uncompressed_size(&self) -> usize {
        self.state.uncomp_size
    }

    pub fn compressed_size(&self) -> usize {
        self.state.comp_size
    }

    pub fn inner_iter<'i, 'o>(
        &'o mut self,
        mut input: &'i [u8],
        mut callback: impl FnMut(&[u8]),
    ) -> Result<State<'i, 'static>, TINFLStatus> {
        loop {
            let state = self.feed_input(input)?;
            match state {
                State::HasOutput {
                    unparsed_input,
                    output,
                } => {
                    input = unparsed_input;
                    callback(output);
                }
                State::NeedsInput { unparsed_input } => {
                    return Ok(State::NeedsInput { unparsed_input })
                }
                State::Stop { unparsed_input } => return Ok(State::Stop { unparsed_input }),
            }
        }
    }

    pub fn try_inner_iter<'i, 'o, E>(
        &'o mut self,
        mut input: &'i [u8],
        mut callback: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<State<'i, 'o>, InnerIterError<E>> {
        loop {
            let state = self.feed_input(input)?;
            match state {
                State::HasOutput {
                    unparsed_input,
                    output,
                } => {
                    input = unparsed_input;
                    match callback(output) {
                        Ok(()) => (),
                        Err(e) => return Err(InnerIterError::UserErr(e)),
                    }
                }
                State::NeedsInput { unparsed_input } => {
                    return Ok(State::NeedsInput { unparsed_input })
                }
                State::Stop { unparsed_input } => return Ok(State::Stop { unparsed_input }),
            }
        }
    }
}

#[derive(Debug)]
pub enum InnerIterError<E> {
    UserErr(E),
    IterErr(TINFLStatus),
}

impl<E> From<TINFLStatus> for InnerIterError<E> {
    fn from(from: TINFLStatus) -> InnerIterError<E> {
        InnerIterError::IterErr(from)
    }
}
/*
impl<'a> ChunkIter<'a> {
    pub fn get(&self) -> &[u8]

    pub fn next(self) -> Result<State<'a>, TINFLStatus> {
        // This happens if there's more to decode than fits the end of the output buffer,
        // and the decoder continues from the start.
        // In that case, we can't return a continuous slice to the output, so we split it to
        // two ChunkIters, and return the rest of the message when calling next again.
        if 0 < self.state.out_pos && self.state.out_pos <= self.state.last_out_pos {
            return Ok(State::HasOutput(self));
        }

        // This happens when the stream has ended, but there was still a final piece of output
        // that needs to be returned.
        if !self.stream_continues {
            return Ok(State::Stop(
                self.input,
                FinishedStream {
                    uncomp_size: self.state.uncomp_size,
                    comp_size: self.state.comp_size,
                    flags: self.state.flags,
                },
            ));
        }

        // This happens when the stream needs more input, but ChunkIter was still returned
        // to hand out the last output before more input is needed.
        if self.input.is_empty() {
            return Ok(State::NeedsInput(InputSink { state: self.state }));
        }

        // At this point we know that the stream is still continuing,
        // and we do not have a dangling "split end" of a message waiting to be delivered
        // and we still don't need more input, but instead need to decode what we currently have.

        consume_input(self.input, self.state)
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_decompression_partial_repetitive_text() {
        use std::str::from_utf8;

        let data_zip = fs::read("tests/assets/zip/repetitive_data.txt.zip").unwrap();
        let pure_deflate_stream = &data_zip[65..670];

        let expected = fs::read_to_string("tests/assets/uncompressed/repetitive_data.txt").unwrap();

        fn test_with_chunk_size(size: usize, deflate_stream: &[u8], expected: &str) {
            let mut stream = start_deflate_stream();
            let mut out_pos = 0;

            for chunk in deflate_stream.chunks(size) {
                println!("Input chunk length: {} bytes", chunk.len());

                let mut state = stream.feed_input(chunk).unwrap();

                while let State::HasOutput {
                    unparsed_input,
                    output,
                } = state
                {
                    {
                        println!("Buffer length: {} Pos: {}", output.len(), out_pos);
                        assert_eq!(
                            from_utf8(output).unwrap(),
                            &expected[out_pos..out_pos + output.len()]
                        );
                        out_pos += output.len();
                        println!("Matches with expected!");
                    }
                    state = stream.feed_input(unparsed_input).unwrap();
                }
            }
        }

        test_with_chunk_size(50, pure_deflate_stream, &expected);
        test_with_chunk_size(150, pure_deflate_stream, &expected);
        test_with_chunk_size(300, pure_deflate_stream, &expected);
        test_with_chunk_size(500, pure_deflate_stream, &expected);
        test_with_chunk_size(700, pure_deflate_stream, &expected);
    }

    #[test]
    fn test_decompression_partial_repetitive_text_inner_iter() {
        use std::str::from_utf8;

        let data_zip = fs::read("tests/assets/zip/repetitive_data.txt.zip").unwrap();
        let pure_deflate_stream = &data_zip[65..670];

        let expected = fs::read_to_string("tests/assets/uncompressed/repetitive_data.txt").unwrap();

        fn test_with_chunk_size(size: usize, deflate_stream: &[u8], expected: &str) {
            let mut stream = start_deflate_stream();
            let mut out_pos = 0;
            let mut last_state = None;

            for chunk in deflate_stream.chunks(size) {
                println!("Input chunk length: {} bytes", chunk.len());

                let state = stream
                    .inner_iter(chunk, |out: &[u8]| {
                        println!("Buffer length: {} Pos: {}", out.len(), out_pos);
                        assert_eq!(
                            from_utf8(out).unwrap(),
                            &expected[out_pos..out_pos + out.len()]
                        );
                        out_pos += out.len();
                        println!("Matches with expected!");
                    })
                    .unwrap();
                match state {
                    State::NeedsInput { unparsed_input } if unparsed_input.is_empty() => (),
                    State::Stop { unparsed_input } if unparsed_input.is_empty() => {
                        last_state = Some(state)
                    }
                    state => panic!("Un-expected parser state: {:?}", state),
                }
            }
            assert_eq!(
                last_state.unwrap(),
                State::Stop {
                    unparsed_input: &[][..]
                }
            );
        }

        test_with_chunk_size(50, pure_deflate_stream, &expected);
        test_with_chunk_size(150, pure_deflate_stream, &expected);
        test_with_chunk_size(300, pure_deflate_stream, &expected);
        test_with_chunk_size(500, pure_deflate_stream, &expected);
        test_with_chunk_size(700, pure_deflate_stream, &expected);
    }

    #[test]
    fn test_decompression_partial_ultra_repetitive_text() {
        use zip;

        use std::str::from_utf8;

        let data_zip = fs::read("tests/assets/zip/ultra_repetitive_data.txt.zip").unwrap();
        let (unparsed, _parsed_header) =
            zip::headers::LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

        let expected =
            fs::read_to_string("tests/assets/uncompressed/ultra_repetitive_data.txt").unwrap();

        let mut stream = start_deflate_stream();
        let mut out_pos = 0;
        let mut last_state = None;

        for chunk in unparsed.chunks(1024) {
            println!("Input chunk length: {} bytes", chunk.len());

            let state = stream
                .inner_iter(chunk, |out| {
                    println!("Buffer length: {} Pos: {}", out.len(), out_pos);
                    assert_eq!(
                        from_utf8(out).unwrap(),
                        &expected[out_pos..out_pos + out.len()]
                    );
                    out_pos += out.len();
                    println!("Matches with expected!");
                })
                .expect("Should be valid DEFLATE");
            match state {
                State::NeedsInput { unparsed_input } if unparsed_input.is_empty() => (),
                State::Stop { unparsed_input } => last_state = Some(state),
                state => panic!("Un-expected parser state: {:?}", state),
            }
        }
        if let State::Stop { .. } = last_state.unwrap() {
        } else {
            panic!("Un-expected parser state!");
        }
    }

    #[test]
    fn test_decompression_partial_short_data_text() {
        use zip;

        use std::str::from_utf8;

        let data_zip = fs::read("tests/assets/zip/short_data.txt.zip").unwrap();
        let (unparsed, _parsed_header) =
            zip::headers::LocalFileHeader::parse(&data_zip).expect("Should be able to parse");

        let expected = fs::read_to_string("tests/assets/uncompressed/short_data.txt").unwrap();

        let mut stream = start_deflate_stream();
        let mut out_pos = 0;

        println!("Input chunk length: {} bytes", unparsed.len());

        let state = stream
            .inner_iter(unparsed, |out| {
                println!("Buffer length: {} Pos: {}", out.len(), out_pos);
                assert_eq!(
                    from_utf8(out).unwrap(),
                    &expected[out_pos..out_pos + out.len()]
                );
                out_pos += out.len();
                println!("Matches with expected!");
            })
            .expect("Should be valid DEFLATE");

        if let State::Stop { unparsed_input } = state {
            assert_eq!(unparsed_input.len(), 110);
        } else {
            panic!("That should be all, folks!");
        }
    }
}
