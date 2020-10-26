use std::ops::Deref;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Input<'long, 'short>
where
    'long: 'short,
{
    Long(&'long [u8]),
    Short(&'short [u8]),
}

impl<'long, 'short> Deref for Input<'long, 'short>
where
    'long: 'short,
{
    type Target = &'short [u8];

    fn deref(&self) -> &&'short [u8] {
        match self {
            Input::Long(long) => &long,
            Input::Short(short) => &short,
        }
    }
}

impl<'long, 'short> Input<'long, 'short> {
    pub fn assert_take_long(self) -> &'long [u8] {
        match self {
            Input::Long(long) => long,
            Input::Short(_) => panic!("Assert failed! Input contained Short."),
        }
    }
}

pub struct InputHandler<'l> {
    orig_stored: usize,
    input_consumed: usize,
    storage: Vec<u8>,
    orig_input: &'l [u8],
}

impl<'l> InputHandler<'l> {
    pub fn take_storage(reader_storage: &mut Vec<u8>, orig_input: &'l [u8]) -> InputHandler<'l> {
        let mut storage = Vec::new();
        std::mem::swap(reader_storage, &mut storage);
        let orig_stored = storage.len();

        let mut ihandler = InputHandler {
            orig_stored,
            input_consumed: 0,
            storage,
            orig_input,
        };

        let unparsed = if orig_stored > 0 {
            // There was too little input to progress the parser state during
            // the previous call so the input of last call was stored to self.unparsed.
            // Appending the current input to existing unparsed input
            // for them to form a continuous buffer.
            ihandler.extend_input();
        };

        ihandler
    }

    pub fn return_storage(mut self, reader_storage: &mut Vec<u8>) {
        std::mem::swap(reader_storage, &mut self.storage);
    }

    pub fn get_unparsed<'s>(&'s self) -> Input<'l, 's> {
        if self.storage.len() > 0 {
            Input::Short(self.storage.as_slice())
        } else {
            Input::Long(self.orig_input)
        }
    }

    pub fn consumed<'s>(&'s mut self, bytes: usize) -> Input<'l, 's> {
        if bytes == 0 {
            if self.storage.len() > 0 {
                return Input::Short(&self.storage[..]);
            } else {
                return Input::Long(&self.orig_input[self.input_consumed..]);
            }
        }

        // Either no bytes are consumed or then
        // so much is consumed that it exceeds the stored amount.
        // Consuming only a part of the stored amount does not happen,
        // because it was enough to be parsed, it wouldn't have been
        // stored at first place.
        debug_assert!(self.orig_stored < bytes);
        self.input_consumed += bytes - self.orig_stored;
        // Consumed data successfully so the stored data isn't needed anymore.
        self.storage.truncate(0);
        self.orig_stored = 0;
        // We re-assign unparsed with the long `input` lifetime
        // to decouple it from the lifetime of stored data
        Input::Long(&self.orig_input[self.input_consumed..])
    }

    pub fn extend_input(&mut self) -> usize {
        // self.storage may contain bytes that were originally stored,
        // or added there. By subtracting self.orig_stored, the amount
        // of bytes originally stored, we get the amount of input that
        // was added to the storage.
        // Additionally, we want to add the amount of input consumed
        // to get the amount of input bytes that were either
        // consumed OR stored.
        // By indexing this amount as the lower bound, we get the slice
        // of input that hasn't been processed any way yet.
        // We call that "extension".
        let input_stored_consumed = self.storage.len() - self.orig_stored + self.input_consumed;
        let upper_bound = std::cmp::min(input_stored_consumed + 80, self.orig_input.len());
        let extension = &self.orig_input[input_stored_consumed..upper_bound];
        let was_empty = self.storage.is_empty();
        self.storage.extend(extension);
        if was_empty {
            // The storage was empty, which means that even if we extended it,
            // the parser has tried to parse with the original input slice
            // and failed; that means that the input length hasn't extended
            // from the viewpoint of the parser.
            0
        } else {
            extension.len()
        }
    }
}
