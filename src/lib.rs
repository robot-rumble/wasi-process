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
//!
//! You can also enforce a timeout on the process by calling `tokio::time::timeout` on it:
//!
//! ```ignore
//! use tokio::{task, time};
//! match time::timeout(time::Duration::from_secs(5), wasi).await {
//!     Ok(_wasmer_result) => {}
//!     Err(_timeout) => {}
//! }
//! ```
#![deny(missing_docs)]

use pin_project_lite::pin_project;
use std::fmt;
use std::future::Future;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::{io, task};
use wasmer_runtime::{error::CallError, Instance};
use wasmer_wasi::state::WasiStateBuilder;

mod stdio;
pub use stdio::{Stderr, Stdin, Stdout};

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
        .stderr(Box::new(stdio::Stdout))
}

type Buf = Cursor<Vec<u8>>;
type StdinInner = io::Result<Buf>;
tokio::task_local! {
    static STDIN: Arc<Mutex<io::StreamReader<mpsc::Receiver<StdinInner>, Buf>>>;
    static STDOUT: Arc<Mutex<mpsc::Sender<StdinInner>>>;
    static STDERR: Arc<Mutex<mpsc::Sender<StdinInner>>>;
}

/// An AsyncWrite type representing a wasi stdin stream.
pub struct WasiStdin {
    tx: Option<mpsc::Sender<StdinInner>>,
}

impl AsyncWrite for WasiStdin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let tx = match &mut self.tx {
            Some(tx) => tx,
            None => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::Other,
                    "called write after shutdown",
                )))
            }
        };
        tx.poll_ready(cx).map(|res| {
            let kind = io::ErrorKind::BrokenPipe; // ?
            res.map_err(|e| io::Error::new(kind, e))
                .and_then(|()| {
                    tx.try_send(Ok(Cursor::new(buf.to_owned())))
                        .map_err(|e| io::Error::new(kind, e))
                })
                .map(|()| buf.len())
        })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        self.tx.take().map(drop);
        Poll::Ready(Ok(()))
    }
}

pin_project! {
    /// An AsyncRead type representing a wasi stdout stream.
    pub struct WasiStdout {
        #[pin]
        inner: io::StreamReader<mpsc::Receiver<StdinInner>, Buf>,
    }
}
impl AsyncRead for WasiStdout {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_read(cx, buf)
    }
}
pin_project! {
    /// An AsyncRead type representing a wasi stderr stream.
    pub struct WasiStderr {
        #[pin]
        inner: io::StreamReader<mpsc::Receiver<StdinInner>, Buf>,
    }
}
impl AsyncRead for WasiStderr {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_read(cx, buf)
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
    handle: futures::future::BoxFuture<'static, Result<(), CallError>>,
}

impl WasiProcess {
    /// Create a WasiProcess from a wasm instance. See the crate documentation for more details.
    pub fn new(instance: Instance) -> Self {
        let (in_tx, in_rx) = mpsc::channel(5);
        let (out_tx, out_rx) = mpsc::channel(5);
        let (err_tx, err_rx) = mpsc::channel(5);
        let handle = STDIN.scope(
            Arc::new(Mutex::new(io::stream_reader(in_rx))),
            STDOUT.scope(
                Arc::new(Mutex::new(out_tx)),
                STDERR.scope(Arc::new(Mutex::new(err_tx)), async move {
                    task::block_in_place(|| instance.call("_start", &[]).map(drop))
                }),
            ),
        );

        Self {
            stdin: Some(WasiStdin { tx: Some(in_tx) }),
            stdout: Some(WasiStdout {
                inner: io::stream_reader(out_rx),
            }),
            stderr: Some(WasiStderr {
                inner: io::stream_reader(err_rx),
            }),
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
    type Output = Result<(), CallError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.handle.as_mut().poll(cx)
    }
}

pin_project! {
    /// A handle to a spawned a wasi process.
    #[derive(Debug)]
    pub struct SpawnHandle {
        #[pin]
        inner: tokio::task::JoinHandle<<WasiProcess as Future>::Output>,
    }
}

impl Future for SpawnHandle {
    type Output = Result<(), SpawnError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project()
            .inner
            .poll(cx)
            .map(|res| res.map_err(SpawnError::Join)?.map_err(SpawnError::Wasi))
    }
}

/// An error returned from a spawned process. Either an error from tokio's `task::spawn`, such as a
/// panic or cancellation, or a wasm/wasi error, like an `_exit()` call or an unreachable.
#[derive(Debug)]
pub enum SpawnError {
    /// An error received from wasmer
    Wasi(CallError),
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
