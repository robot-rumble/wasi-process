use tokio::prelude::*;
use wasi_process::WasiProcess;
use wasmer_wasi::{state::WasiState, WasiVersion};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let wasm = std::fs::read(args.next().expect("must pass wasm file"))?;
    let module = wasmer_runtime::compile(&wasm)?;
    let mut state = WasiState::new("progg");
    wasi_process::add_stdio(&mut state);
    state.args(args).preopen_dir(".")?;
    let imports = wasmer_wasi::generate_import_object_from_state(
        state.build()?,
        wasmer_wasi::get_wasi_version(&module, false).unwrap_or(WasiVersion::Latest),
    );
    let mut wasi = WasiProcess::new(module.instantiate(&imports)?);
    let mut proc_stdin = wasi.stdin.take().unwrap();
    let mut stdin = io::stdin();
    let mut proc_stdout = wasi.stdout.take().unwrap();
    let mut stdout = io::stdout();
    let mut proc_stderr = wasi.stderr.take().unwrap();
    let mut stderr = io::stderr();
    let proc_fut = wasi.spawn();
    tokio::try_join!(
        io::copy(&mut stdin, &mut proc_stdin),
        io::copy(&mut proc_stdout, &mut stdout),
        io::copy(&mut proc_stderr, &mut stderr),
    )?;
    proc_fut.await?;
    Ok(())
}
