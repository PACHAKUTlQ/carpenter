use std::io::{Read, Write};
use super::{Transformer, TransformerPortState, TransformerResult};
use super::streambuffer::StreamBuffer;


pub struct DirectConnectionTransformer {
    transmit_buf: StreamBuffer,
    receive_buf: StreamBuffer,
    // transmit_closed: bool,
    // receive_closed: bool,
}

impl DirectConnectionTransformer {
    pub fn new() -> Self {
        Self {
            transmit_buf: StreamBuffer::new(),
            receive_buf: StreamBuffer::new(),
            // transmit_closed: true,
            // receive_closed: true,
        }
    }
}

impl Transformer for DirectConnectionTransformer {
    /* transmit tube */

    fn transmit_writable(&self) -> TransformerPortState {
        TransformerPortState::Open(self.transmit_buf.writable_size().try_into().unwrap())
    }

    fn transmit_write(&mut self, buf: &[u8]) -> TransformerResult {
        TransformerResult::Ok(self.transmit_buf.write(buf).unwrap())
    }

    fn transmit_readable(&self) -> TransformerPortState {
        TransformerPortState::Open(self.transmit_buf.readable_size().try_into().unwrap())
    }

    fn transmit_read(&mut self, buf: &mut [u8]) -> TransformerResult {
        TransformerResult::Ok(self.transmit_buf.read(buf).unwrap())
    }

    /* receive tube */

    fn receive_writable(&self) -> TransformerPortState {
        TransformerPortState::Open(self.receive_buf.writable_size().try_into().unwrap())
    }

    fn receive_write(&mut self, buf: &[u8]) -> TransformerResult {
        TransformerResult::Ok(self.receive_buf.write(buf).unwrap())
    }

    fn receive_readable(&self) -> TransformerPortState {
        TransformerPortState::Open(self.receive_buf.readable_size().try_into().unwrap())
    }

    fn receive_read(&mut self, buf: &mut [u8]) -> TransformerResult {
        TransformerResult::Ok(self.receive_buf.read(buf).unwrap())
    }
}

