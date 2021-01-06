//! A unidirectional pipe implementation modified from private module tokio::io::util::mem; it's
//! the backing data structure behind DuplexStream

use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use bytes::{Buf, BytesMut};
use std::{
    pin::Pin,
    sync::Arc,
    task::{self, Poll, Waker},
};

/// A unidirectional IO over a piece of memory.
///
/// Data can be written to the pipe, and reading will return that data.
#[derive(Debug)]
pub struct Pipe {
    /// The buffer storing the bytes written, also read from.
    ///
    /// Using a `BytesMut` because it has efficient `Buf` and `BufMut`
    /// functionality already. Additionally, it can try to copy data in the
    /// same buffer if there read index has advanced far enough.
    buffer: BytesMut,
    /// Determines if the write side has been closed.
    is_closed: bool,
    /// The maximum amount of bytes that can be written before returning
    /// `Poll::Pending`.
    max_buf_size: usize,
    /// If the `read` side has been polled and is pending, this is the waker
    /// for that parked task.
    read_waker: Option<Waker>,
    /// If the `write` side has filled the `max_buf_size` and returned
    /// `Poll::Pending`, this is the waker for that parked task.
    write_waker: Option<Waker>,
}

#[derive(Debug, Clone)]
pub struct LockPipe {
    inner: Arc<Mutex<Pipe>>,
}

impl Pipe {
    pub fn new(max_buf_size: usize) -> Self {
        Pipe {
            buffer: BytesMut::new(),
            is_closed: false,
            max_buf_size,
            read_waker: None,
            write_waker: None,
        }
    }

    fn close(&mut self) {
        self.is_closed = true;
        if let Some(waker) = self.read_waker.take() {
            waker.wake();
        }
    }
}

impl AsyncRead for Pipe {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.buffer.has_remaining() {
            let max = self.buffer.remaining().min(buf.remaining());
            buf.put_slice(&self.buffer[..max]);
            self.buffer.advance(max);
            if max > 0 {
                // The passed `buf` might have been empty, don't wake up if
                // no bytes have been moved.
                if let Some(waker) = self.write_waker.take() {
                    waker.wake();
                }
            }
            Poll::Ready(Ok(()))
        } else if self.is_closed {
            Poll::Ready(Ok(()))
        } else {
            self.read_waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl AsyncWrite for Pipe {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.is_closed {
            return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
        }
        let avail = self.max_buf_size - self.buffer.len();
        if avail == 0 {
            self.write_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }

        let len = buf.len().min(avail);
        self.buffer.extend_from_slice(&buf[..len]);
        if let Some(waker) = self.read_waker.take() {
            waker.wake();
        }
        Poll::Ready(Ok(len))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut task::Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        _: &mut task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.close();
        Poll::Ready(Ok(()))
    }
}

impl LockPipe {
    pub fn new(max_buf_size: usize) -> Self {
        let inner = Arc::new(Mutex::new(Pipe::new(max_buf_size)));
        Self { inner }
    }
}

impl AsyncRead for LockPipe {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.inner.lock()).poll_read(cx, buf)
    }
}

impl AsyncWrite for LockPipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut *self.inner.lock()).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.inner.lock()).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.inner.lock()).poll_shutdown(cx)
    }
}

impl Drop for LockPipe {
    fn drop(&mut self) {
        // notify the other side of the closure
        self.inner.lock().close();
    }
}
