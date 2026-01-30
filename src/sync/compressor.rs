use crate::sync::error::SyncError;
use std::io::{Read, Write};

pub struct StreamingCompressor {
    level: i32,
    threads: u32,
}

impl StreamingCompressor {
    pub fn new(level: u8, threads: u8) -> Self {
        let threads = if threads == 0 {
            num_cpus::get() as u32
        } else {
            threads as u32
        };

        Self {
            level: level as i32,
            threads,
        }
    }

    /// Compress data from reader to writer using multi-threaded zstd
    pub fn compress<R: Read, W: Write>(
        &self,
        mut reader: R,
        writer: W,
    ) -> Result<u64, SyncError> {
        let mut encoder = zstd::stream::Encoder::new(writer, self.level)
            .map_err(|e| SyncError::CompressError(format!("create encoder: {e}")))?;

        // Enable multi-threaded compression
        encoder
            .multithread(self.threads)
            .map_err(|e| SyncError::CompressError(format!("set threads: {e}")))?;

        let bytes_written = std::io::copy(&mut reader, &mut encoder)
            .map_err(|e| SyncError::CompressError(format!("compress: {e}")))?;

        encoder
            .finish()
            .map_err(|e| SyncError::CompressError(format!("finish: {e}")))?;

        Ok(bytes_written)
    }
}
