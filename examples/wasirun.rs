use tokio::io;
use wasi_process::WasiProcess;
use wasmer_wasi::{WasiEnv, WasiState, WasiVersion};

type Error = Box<dyn std::error::Error>;

fn start_wasi_process(store: &wasmer::Store) -> Result<WasiProcess, Error> {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("must pass wasm file");
    let module = wasmer::Module::from_file(&store, path)?;
    let mut state = WasiState::new("progg");
    wasi_process::add_stdio(&mut state);
    state.args(args).preopen_dir(".")?;
    let env = WasiEnv::new(state.build()?);
    let version = wasmer_wasi::get_wasi_version(&module, false).unwrap_or(WasiVersion::Latest);
    let imports = wasmer_wasi::generate_import_object_from_env(&store, env, version);
    let instance = wasmer::Instance::new(&module, &imports)?;
    let wasi = WasiProcess::new(&instance, wasi_process::MaxBufSize::default())?;
    Ok(wasi)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let store = wasmer::Store::default();
    let mut wasi = start_wasi_process(&store)?;
    let mut proc_stdin = wasi.stdin.take().unwrap();
    let mut stdin = io::stdin();
    let mut proc_stdout = wasi.stdout.take().unwrap();
    let mut stdout = io::stdout();
    let mut proc_stderr = wasi.stderr.take().unwrap();
    let mut stderr = io::stderr();
    let proc_fut = wasi.spawn();
    tokio::try_join!(
        async {
            io::copy(&mut stdin, &mut proc_stdin).await?;
            println!("hey");
            Ok::<_, Error>(())
        },
        async {
            io::copy(&mut proc_stdout, &mut stdout).await?;
            println!("ho");
            Ok::<_, Error>(())
        },
        async {
            io::copy(&mut proc_stderr, &mut stderr).await?;
            println!("letsgo");
            Ok::<_, Error>(())
        },
    )?;
    proc_fut.await?;
    Ok(())
}
