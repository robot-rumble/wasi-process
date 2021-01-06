use serde::{Deserialize, Serialize};
use std::io::{prelude::*, SeekFrom};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use wasmer_wasi::{types as wasi_types, WasiFile, WasiFsError};

use super::{STDERR, STDIN, STDOUT};

// TODO: replace with tokio::runtime::Handle::current().block_on() if that's ever added
use futures_executor::block_on;

/// The stdin pseudo-file for wasi processes.
#[derive(Debug, Serialize, Deserialize)]
pub struct Stdin;
impl Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        block_on(async {
            let mut stdin = STDIN.with(Clone::clone);
            stdin.read(buf).await
        })
    }
}
impl Seek for Stdin {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(io::ErrorKind::Other, "can not seek stdin"))
    }
}
impl Write for Stdin {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not write to stdin",
        ))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not write to stdin",
        ))
    }
    fn write_all(&mut self, _buf: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not write to stdin",
        ))
    }
    fn write_fmt(&mut self, _fmt: ::std::fmt::Arguments) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not write to stdin",
        ))
    }
}

#[typetag::serde(name = "tokio_process_stdin")]
impl WasiFile for Stdin {
    fn last_accessed(&self) -> u64 {
        0
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn created_time(&self) -> u64 {
        0
    }
    fn size(&self) -> u64 {
        0
    }
    fn set_len(&mut self, _new_size: wasi_types::__wasi_filesize_t) -> Result<(), WasiFsError> {
        Err(WasiFsError::PermissionDenied)
    }

    fn unlink(&mut self) -> Result<(), WasiFsError> {
        Ok(())
    }

    fn bytes_available(&self) -> Result<usize, WasiFsError> {
        Ok(0)
    }

    fn get_raw_fd(&self) -> Option<i32> {
        None
    }
}

/// The stdout pseudo-file for wasi processes.
#[derive(Debug, Serialize, Deserialize)]
pub struct Stdout;
impl Read for Stdout {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stdout",
        ))
    }
    fn read_to_end(&mut self, _buf: &mut Vec<u8>) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stdout",
        ))
    }
    fn read_to_string(&mut self, _buf: &mut String) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stdout",
        ))
    }
    fn read_exact(&mut self, _buf: &mut [u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stdout",
        ))
    }
}
impl Seek for Stdout {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(io::ErrorKind::Other, "can not seek stdout"))
    }
}
impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        block_on(async move {
            let mut stdout = STDOUT.with(Clone::clone);
            stdout.write(buf).await
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[typetag::serde(name = "tokio_process_stdout")]
impl WasiFile for Stdout {
    fn last_accessed(&self) -> u64 {
        0
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn created_time(&self) -> u64 {
        0
    }
    fn size(&self) -> u64 {
        0
    }
    fn set_len(&mut self, _new_size: wasi_types::__wasi_filesize_t) -> Result<(), WasiFsError> {
        Err(WasiFsError::PermissionDenied)
    }
    fn unlink(&mut self) -> Result<(), WasiFsError> {
        Ok(())
    }

    fn bytes_available(&self) -> Result<usize, WasiFsError> {
        Err(WasiFsError::InvalidInput)
    }

    fn get_raw_fd(&self) -> Option<i32> {
        None
    }
}

/// The stderr pseudo-file for wasi processes.
#[derive(Debug, Serialize, Deserialize)]
pub struct Stderr;
impl Read for Stderr {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stderr",
        ))
    }
    fn read_to_end(&mut self, _buf: &mut Vec<u8>) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stderr",
        ))
    }
    fn read_to_string(&mut self, _buf: &mut String) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stderr",
        ))
    }
    fn read_exact(&mut self, _buf: &mut [u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "can not read from stderr",
        ))
    }
}
impl Seek for Stderr {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(io::ErrorKind::Other, "can not seek stderr"))
    }
}
impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        block_on(async move {
            let mut stderr = STDERR.with(Clone::clone);
            stderr.write(buf).await
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[typetag::serde(name = "tokio_process_stderr")]
impl WasiFile for Stderr {
    fn last_accessed(&self) -> u64 {
        0
    }
    fn last_modified(&self) -> u64 {
        0
    }
    fn created_time(&self) -> u64 {
        0
    }
    fn size(&self) -> u64 {
        0
    }
    fn set_len(&mut self, _new_size: wasi_types::__wasi_filesize_t) -> Result<(), WasiFsError> {
        Err(WasiFsError::PermissionDenied)
    }
    fn unlink(&mut self) -> Result<(), WasiFsError> {
        Ok(())
    }

    fn bytes_available(&self) -> Result<usize, WasiFsError> {
        Err(WasiFsError::InvalidInput)
    }

    fn get_raw_fd(&self) -> Option<i32> {
        None
    }
}
