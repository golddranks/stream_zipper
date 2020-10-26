extern crate core;
extern crate miniz_oxide;
extern crate nom;

pub mod deflate;
pub mod gzip;
pub mod input_helper;
pub mod utils;
pub mod zip;

trait CompressedStream: Sized {
    fn feed_input(&mut self, input: &[u8]) -> State<Self>;
}

/// Represents a state of a compressed input stream.
/// Generic over the actual stream type (gzip or zip).
pub enum State<'i, 's, File>
where
    'i: 's,
{
    NeedsInputOrEof(gzip::GZipFile),
    NeedsInput,
    HasOutput {
        unparsed_input: &'i [u8],
        output: &'s [u8],
    },
    NextFile {
        unparsed_input: &'i [u8],
        next_file: File,
    },
    EndOfFile,
}

impl<'i, 's, F> State<'i, 's, F> {
    pub fn assert_no_output(self) -> State<'i, 'i, F> {
        use State::*;
        match self {
            NeedsInputOrEof(f) => NeedsInputOrEof(f),
            NeedsInput => NeedsInput,
            HasOutput { .. } => panic!("Assertion failed: self was HasOutput"),
            NextFile {
                unparsed_input,
                next_file,
            } => NextFile {
                unparsed_input,
                next_file,
            },
            EndOfFile => EndOfFile,
        }
    }
}

impl<'i, 's, File> std::fmt::Debug for State<'i, 's, File> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        use State::*;
        match self {
            NeedsInputOrEof(_) => writeln!(f, "State::NeedsInputOrEof"),
            NeedsInput => writeln!(f, "State::NeedsInput"),
            HasOutput { .. } => writeln!(f, "State::HasOutput"),
            NextFile { .. } => writeln!(f, "State::NextFile"),
            EndOfFile => writeln!(f, "State::EndOfFile"),
        }
    }
}

pub enum ReadHeadersResult<'i> {
    NeedsInput,
    Done { unparsed: &'i [u8] },
}

impl<'i, 's> From<State<'i, 's, zip::ZipFile>> for State<'i, 's, File> {
    fn from(from: State<'i, 's, zip::ZipFile>) -> State<'i, 's, File> {
        use State::*;

        match from {
            NeedsInputOrEof(_) => unreachable!(
                "Zip files have always directory at end so we know if we have reached the end."
            ),
            NeedsInput => NeedsInput,
            HasOutput {
                unparsed_input,
                output,
            } => HasOutput {
                unparsed_input,
                output,
            },
            NextFile {
                unparsed_input,
                next_file,
            } => NextFile {
                unparsed_input,
                next_file: next_file.into(),
            },
            EndOfFile => EndOfFile,
        }
    }
}

impl<'i, 's> From<State<'i, 's, gzip::GZipFile>> for State<'i, 's, File> {
    fn from(from: State<'i, 's, gzip::GZipFile>) -> State<'i, 's, File> {
        use State::*;

        match from {
            NeedsInputOrEof(file) => NeedsInputOrEof(file),
            NeedsInput => NeedsInput,
            HasOutput {
                unparsed_input,
                output,
            } => HasOutput {
                unparsed_input,
                output,
            },
            NextFile {
                unparsed_input,
                next_file,
            } => NextFile {
                unparsed_input,
                next_file: next_file.into(),
            },
            EndOfFile => EndOfFile,
        }
    }
}

/// Corresponds to a zipped or gzipped file/stream.
/// Can be in one of three states: not-yet-detected type, a zip file or a gzip file.
pub enum File {
    Zip(zip::ZipFile),
    GZip(gzip::GZipFile),
    Init(Vec<u8>),
}

enum AutodetectResult {
    NeedsMoreData,
    UnknownFormat,
    Detected(File),
}

impl File {
    pub fn name(&self) -> Option<&[u8]> {
        use File::*;

        match self {
            Zip(zip) => zip.filename(),
            GZip(gzip) => gzip.filename(),
            Init(_) => None,
        }
    }

    /// Reads the fist 4 bytes of the input and tries to autodetect the stream format.
    /// Consumes and retains the amount of bytes read from input in `unparsed` buffer.
    /// Once the detection succeeds, constructs a stream object of the detected format
    /// and feeds it the consumed first bytes.
    /// In case where there is no enough data for detection,
    /// it consumes the input it can and returns,
    /// expecting to be called again with more data.
    fn autodetect_format(unparsed: &mut Vec<u8>, input: &mut &[u8]) -> AutodetectResult {
        const NEEDED_BYTES: usize = 4;
        if unparsed.len() + input.len() < NEEDED_BYTES {
            unparsed.extend_from_slice(input);
            *input = &[][..];
            return AutodetectResult::NeedsMoreData;
        } else {
            // Byte count from start of input that are used for autodetection
            let bytes_to_consume = NEEDED_BYTES - unparsed.len();
            unparsed.extend_from_slice(&input[..bytes_to_consume]);
            *input = &input[bytes_to_consume..];
        }

        // Bytes needed for detection are now in `unparsed`!

        // Start a stream according to a detected stream type
        // and feed in the first bytes
        // that where used for detection.
        if unparsed.starts_with(b"\x50\x4b\x03\x04") {
            let mut stream = zip::start_stream();
            stream
                .read(unparsed)
                .expect("No errors will happen with the 4 first input bytes.");
            AutodetectResult::Detected(File::Zip(stream))
        } else if unparsed.starts_with(b"\x1f\x8b\x08") {
            let mut stream = gzip::start_stream();
            stream
                .read(unparsed)
                .expect("No errors will happen with the 4 first input bytes.");
            AutodetectResult::Detected(File::GZip(stream))
        } else {
            return AutodetectResult::UnknownFormat;
        }
    }

    pub fn get_output(&self) -> &[u8] {
        use File::*;
        match self {
            Zip(file) => file.get_output(),
            GZip(file) => file.get_output(),
            Init(file) => panic!("This shouldn't be called before autodetect!"),
        }
    }

    pub fn read_headers<'i>(
        &mut self,
        mut input: &'i [u8],
    ) -> Result<ReadHeadersResult<'i>, Error> {
        use File::*;

        // Format detection will run only when the stream has started (the Init state)
        if let Init(ref mut unparsed) = self {
            // Set self to the corresponding format
            *self = match Self::autodetect_format(unparsed, &mut input) {
                AutodetectResult::NeedsMoreData => return Ok(ReadHeadersResult::NeedsInput),
                AutodetectResult::UnknownFormat => return Err(Error::UnknownFileFormat),
                AutodetectResult::Detected(file) => file,
            };
        };

        match self {
            Zip(ref mut file) => Ok(file.read_headers(input)?.into()),
            GZip(ref mut file) => unimplemented!("TODO"),
            Init(_) => {
                unreachable!("The File::Init state is never set after autodetect has succeeded.")
            }
        }
    }

    pub fn read_internal_iter<'i>(
        &mut self,
        mut input: &'i [u8],
        mut callback: impl FnMut(&[u8]),
    ) -> Result<State<'i, 'i, File>, Error> {
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

    pub fn read<'i, 's>(&'s mut self, mut input: &'i [u8]) -> Result<State<'i, 's, File>, Error> {
        use File::*;

        // Format detection will run only when the stream has started (the Init state)
        if let Init(ref mut unparsed) = self {
            // Set self to the corresponding format
            *self = match Self::autodetect_format(unparsed, &mut input) {
                AutodetectResult::NeedsMoreData => return Ok(State::NeedsInput),
                AutodetectResult::UnknownFormat => return Err(Error::UnknownFileFormat),
                AutodetectResult::Detected(file) => file,
            };
        };

        match self {
            Zip(ref mut file) => Ok(file.read(input)?.into()),
            GZip(ref mut file) => Ok(file.read(input)?.into()),
            Init(_) => {
                unreachable!("The File::Init state is never set after autodetect has succeeded.")
            }
        }
    }
}

impl From<zip::ZipFile> for File {
    fn from(f: zip::ZipFile) -> File {
        File::Zip(f)
    }
}

impl From<gzip::GZipFile> for File {
    fn from(f: gzip::GZipFile) -> File {
        File::GZip(f)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use Error::*;
        match self {
            Zip(e) => {
                write!(f, "zip error:")?;
                e.fmt(f)?
            }
            GZip(e) => {
                write!(f, "gzip error:")?;
                e.fmt(f)?
            }
            UnknownFileFormat => write!(f, "no known fileformat (zip or gzip) detected")?,
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Zip(e) => Some(e),
            Self::GZip(e) => Some(e),
            Self::UnknownFileFormat => None,
        }
    }
}

/// An error type that delegates to ZipError or GzipError.
/// In case the file format detection fails, there's a third
/// error state for that.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    Zip(zip::ZipError),
    GZip(gzip::GZipError),
    UnknownFileFormat,
}

impl From<zip::ZipError> for Error {
    fn from(err: zip::ZipError) -> Error {
        Error::Zip(err)
    }
}

impl From<gzip::GZipError> for Error {
    fn from(err: gzip::GZipError) -> Error {
        Error::GZip(err)
    }
}

/// Initialises a File that starts in a state that is agnostic
/// about the whether the input
/// stream is in zip format or gzip format.
/// Use this function to initialise the stream if you want to
/// auto-detect the input format.
pub fn start_stream() -> File {
    File::Init(Vec::new())
}
