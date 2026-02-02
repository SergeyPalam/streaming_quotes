use std::collections::VecDeque;
use anyhow::{Result, bail};
use std::io::{Read, ErrorKind};

#[derive(Default)]
pub struct StreamReader{
    buf: VecDeque<u8>,
}

impl StreamReader {
    pub fn read_from_stream<T: Read>(&mut self, stream: &mut T) -> Result<()> {
        let mut buf = vec![0u8; 512];
        
        match stream.read(&mut buf) {
            Ok(len) => {
                for i in 0..len {
                    self.buf.push_back(buf[i]);
                }
                return Ok(());
            }
            Err(e) => {
                match e.kind() {
                    ErrorKind::WouldBlock | ErrorKind::UnexpectedEof => return Ok(()),
                    _ => bail!("{e}"),
                }
            }
        }
    }

    pub fn extract_chunk(&mut self, chunk_len: usize) -> Option<Vec<u8>> {
        if self.buf.len() < chunk_len {
            return None;
        }

        let mut res = Vec::with_capacity(chunk_len);
        for _ in 0..chunk_len {
            res.push(self.buf.pop_front()?);
        }
        Some(res)
    } 
}