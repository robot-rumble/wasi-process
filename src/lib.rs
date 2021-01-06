//! A library to run wasi modules as pseudo-processes.
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use tokio::prelude::*; tokio::runtime::Runtime::new().unwrap().block_on(async {
//! use wasmer_wasi::{WasiVersion, state::WasiState};
//! use wasi_process::WasiProcess;
//! let wasm = include_bytes!("../helloworld.wasm"); // just write(1, "Hello, World!\n", 14)
//! let module = wasmer_runtime::compile(wasm)?;
//! let mut state = WasiState::new("progg");
//! wasi_process::add_stdio(&mut state);
//! state.args(&["foo", "bar"]);
//! let imports = wasmer_wasi::generate_import_object_from_state(
//!     state.build()?,
//!     wasmer_wasi::get_wasi_version(&module, false).unwrap_or(WasiVersion::Latest),
//! );
//! let mut wasi = WasiProcess::new(module.instantiate(&imports)?);
//! let mut stdout = wasi.stdout.take().unwrap();
//! wasi.spawn();
//! let mut out = String::new();
//! stdout.read_to_string(&mut out).await?;
//! assert_eq!(out, "Hello, World!\n");
//! # Ok(()) })
//! # }
//! ```
#![deny(missing_docs)]

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::{io, task};
use wasmer::RuntimeError;
use wasmer_wasi::WasiStateBuilder;

mod pipe;
mod stdio;

pub use stdio::{Stderr, Stdin, Stdout};

use pipe::LockPipe;

/// Use the wasi-process stdio pseudo-files for a wasi environment.
///
/// # Examples
/// ```
/// # fn main() -> Result<(), wasmer_wasi::state::WasiStateCreationError> {
/// use wasmer_wasi::state::WasiState;
/// let mut state = WasiState::new("programname");
/// wasi_process::add_stdio(&mut state);
/// let state = state.arg("foo").build()?;
/// # let _ = state;
/// # Ok(())
/// # }
/// ```
pub fn add_stdio(state: &mut WasiStateBuilder) -> &mut WasiStateBuilder {
    state
        .stdin(Box::new(stdio::Stdin))
        .stdout(Box::new(stdio::Stdout))
        .stderr(Box::new(stdio::Stderr))
}

tokio::task_local! {
    static STDIN: LockPipe;
    static STDOUT: LockPipe;
    static STDERR: LockPipe;
}

/// An AsyncWrite type representing a wasi stdin stream.
pub struct WasiStdin {
    inner: LockPipe,
}

impl AsyncWrite for WasiStdin {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// An AsyncRead type representing a wasi stdout stream.
pub struct WasiStdout {
    inner: LockPipe,
}
impl AsyncRead for WasiStdout {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

/// An AsyncRead type representing a wasi stderr stream.
pub struct WasiStderr {
    inner: LockPipe,
}
impl AsyncRead for WasiStderr {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

/// A wasi process. See crate documentation for more details and examples.
#[must_use = "WasiProcess does nothing without being polled or spawned. Try calling `.spawn()`"]
pub struct WasiProcess {
    /// An stdin reader for the wasi process
    pub stdin: Option<WasiStdin>,
    /// An stdout writer for the wasi process
    pub stdout: Option<WasiStdout>,
    /// An stderr writer for the wasi process
    pub stderr: Option<WasiStderr>,
    handle: Pin<Box<dyn Future<Output = Result<(), RuntimeError>> + Send + Sync>>,
}

impl WasiProcess {
    /// Create a WasiProcess from a wasm instance. See the crate documentation for more details.
    /// Errors if the instance doesn't have a `_start` function exported.
    pub fn new(
        instance: &wasmer::Instance,
        max_buf_size: usize,
    ) -> Result<Self, wasmer::ExportError> {
        let start = instance.exports.get_function("_start")?.clone();
        Ok(Self::with_function(start, max_buf_size))
    }

    /// Create a WasiProcess from a wasm instance, given a `_start` function. See the crate
    /// documentation for more details.
    pub fn with_function(start_function: wasmer::Function, max_buf_size: usize) -> Self {
        let stdin = LockPipe::new(max_buf_size);
        let stdout = LockPipe::new(max_buf_size);
        let stderr = LockPipe::new(max_buf_size);
        let handle = STDIN.scope(
            stdin.clone(),
            STDOUT.scope(
                stdout.clone(),
                STDERR.scope(stderr.clone(), async move {
                    task::block_in_place(|| start_function.call(&[]).map(drop))
                }),
            ),
        );

        Self {
            stdin: Some(WasiStdin { inner: stdin }),
            stdout: Some(WasiStdout { inner: stdout }),
            stderr: Some(WasiStderr { inner: stderr }),
            handle: Box::pin(handle),
        }
    }

    /// Spawn the process on a tokio task. It's okay to let this drop; that just means that you
    /// don't care about exactly when or how the process finishes, and you'll know you're done when
    /// an stdio stream closes;
    pub fn spawn(self) -> SpawnHandle {
        let inner = tokio::spawn(self);
        SpawnHandle { inner }
    }
}

impl Future for WasiProcess {
    type Output = Result<(), RuntimeError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.handle.as_mut().poll(cx)
    }
}

/// A handle to a spawned a wasi process.
#[derive(Debug)]
pub struct SpawnHandle {
    inner: tokio::task::JoinHandle<<WasiProcess as Future>::Output>,
}

impl Future for SpawnHandle {
    type Output = Result<(), SpawnError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Pin::new(&mut self.inner)
            .poll(cx)
            .map(|res| res.map_err(SpawnError::Join)?.map_err(SpawnError::Wasi))
    }
}

/// An error returned from a spawned process. Either an error from tokio's `task::spawn`, such as a
/// panic or cancellation, or a wasm/wasi error, like an `_exit()` call or an unreachable.
#[derive(Debug)]
pub enum SpawnError {
    /// An error received from wasmer
    Wasi(RuntimeError),
    /// An error from `tokio::task::spawn`
    Join(tokio::task::JoinError),
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Wasi(w) => write!(f, "runtime wasi/wasm error: {}", w),
            Self::Join(j) => write!(f, "error while joining the tokio task: {}", j),
        }
    }
}

impl std::error::Error for SpawnError {}
