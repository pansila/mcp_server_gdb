pub mod commands;
pub mod output;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::Result;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Sender};
use tracing::{debug, info};

use crate::error::{AppError, AppResult};
use output::process_output;

#[allow(clippy::upper_case_acronyms)]
pub struct GDB {
    pub process: Arc<Mutex<Child>>,
    is_running: Arc<AtomicBool>,
    result_output: mpsc::Receiver<output::ResultRecord>,
    current_command_token: AtomicU64,
    binary_path: PathBuf,
    init_options: Vec<OsString>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecuteError {
    Busy,
    Quit,
}

/// A builder struct for configuring and launching GDB with various command line options.
/// This struct provides a fluent interface for setting up GDB with different parameters
/// before spawning the debugger process.
pub struct GDBBuilder {
    /// Path to the GDB executable
    pub gdb_path: PathBuf,
    /// Do not read ~/.gdbinit file (--nh)
    pub opt_nh: bool,
    /// Do not read any .gdbinit files in any directory (--nx)
    pub opt_nx: bool,
    /// Do not print version number on startup (--quiet)
    pub opt_quiet: bool,
    /// Change current directory to DIR (--cd=DIR)
    pub opt_cd: Option<PathBuf>,
    /// Set serial port baud rate used for remote debugging (-b BAUDRATE)
    pub opt_bps: Option<u32>,
    /// Read symbols from SYMFILE (--symbols=SYMFILE)
    pub opt_symbol_file: Option<PathBuf>,
    /// Analyze the core dump COREFILE (--core=COREFILE)
    pub opt_core_file: Option<PathBuf>,
    /// Attach to running process PID (--pid=PID)
    pub opt_proc_id: Option<u32>,
    /// Execute GDB commands from FILE (--command=FILE)
    pub opt_command: Option<PathBuf>,
    /// Search for source files in DIR (--directory=DIR)
    pub opt_source_dir: Option<PathBuf>,
    /// Arguments to be passed to the inferior program (--args)
    pub opt_args: Vec<OsString>,
    /// The executable file to debug
    pub opt_program: Option<PathBuf>,
    /// Use TTY for input/output by the program being debugged (--tty=TTY)
    pub opt_tty: Option<PathBuf>,
}

impl GDBBuilder {
    pub fn new(gdb: PathBuf) -> Self {
        GDBBuilder {
            gdb_path: gdb,
            opt_nh: false,
            opt_nx: false,
            opt_quiet: false,
            opt_cd: None,
            opt_bps: None,
            opt_symbol_file: None,
            opt_core_file: None,
            opt_proc_id: None,
            opt_command: None,
            opt_source_dir: None,
            opt_args: Vec::new(),
            opt_program: None,
            opt_tty: None,
        }
    }

    pub fn try_spawn(self, oob_sink: Sender<output::OutOfBandRecord>) -> AppResult<GDB> {
        let mut gdb_args = Vec::<OsString>::new();
        let mut init_options = Vec::<OsString>::new();
        if self.opt_nh {
            gdb_args.push("--nh".into());
            init_options.push("--nh".into());
        }
        if self.opt_nx {
            gdb_args.push("--nx".into());
            init_options.push("--nx".into());
        }
        if self.opt_quiet {
            gdb_args.push("--quiet".into());
        }
        if let Some(cd) = self.opt_cd {
            gdb_args.push("--cd=".into());
            gdb_args.last_mut().unwrap().push(&cd);
        }
        if let Some(bps) = self.opt_bps {
            gdb_args.push("-b".into());
            gdb_args.push(bps.to_string().into());
        }
        if let Some(symbol_file) = self.opt_symbol_file {
            gdb_args.push("--symbols=".into());
            gdb_args.last_mut().unwrap().push(&symbol_file);
        }
        if let Some(core_file) = self.opt_core_file {
            gdb_args.push("--core=".into());
            gdb_args.last_mut().unwrap().push(&core_file);
        }
        if let Some(proc_id) = self.opt_proc_id {
            gdb_args.push("--pid=".into());
            gdb_args.last_mut().unwrap().push(proc_id.to_string());
        }
        if let Some(command) = self.opt_command {
            gdb_args.push("--command=".into());
            gdb_args.last_mut().unwrap().push(&command);
        }
        if let Some(source_dir) = self.opt_source_dir {
            gdb_args.push("--directory=".into());
            gdb_args.last_mut().unwrap().push(&source_dir);
        }
        if let Some(tty) = self.opt_tty {
            gdb_args.push("--tty=".into());
            gdb_args.last_mut().unwrap().push(&tty);
        }
        if !self.opt_args.is_empty() {
            gdb_args.push("--args".into());
            gdb_args.push(
                self.opt_program
                    .ok_or(AppError::InvalidArgument(
                        "Program path is required if --args is provided".to_string(),
                    ))?
                    .into_os_string(),
            );
            for arg in self.opt_args {
                gdb_args.push(arg);
            }
        } else if let Some(program) = self.opt_program {
            gdb_args.push(program.into());
        }

        let mut command = Command::new(self.gdb_path.clone());
        command.arg("--interpreter=mi").args(gdb_args);

        debug!("Starting GDB process with command: {:?}", command);

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::GDBError(format!("Failed to start GDB process: {}", e)))?;

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let is_running = Arc::new(AtomicBool::new(false));
        let is_running_clone = is_running.clone();
        let (result_input, result_output) = mpsc::channel(100);
        tokio::spawn(process_output(
            stdout,
            result_input,
            oob_sink,
            is_running_clone,
        ));

        let gdb = GDB {
            process: Arc::new(Mutex::new(child)),
            is_running,
            current_command_token: AtomicU64::new(0),
            binary_path: self.gdb_path,
            init_options,
            result_output,
        };
        Ok(gdb)
    }
}

impl GDB {
    #[cfg(unix)]
    pub async fn interrupt_execution(&self) -> Result<(), nix::Error> {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(
            Pid::from_raw(self.process.lock().await.id().unwrap() as i32),
            signal::SIGINT,
        )
    }

    #[cfg(windows)]
    pub async fn interrupt_execution(&self) -> Result<()> {
        Ok(())
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    pub fn init_options(&self) -> &[OsString] {
        &self.init_options
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn new_token(&mut self) -> u64 {
        self.current_command_token.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn execute<C: std::borrow::Borrow<commands::MiCommand>>(
        &mut self,
        command: C,
    ) -> AppResult<output::ResultRecord> {
        if self.is_running() {
            return Err(AppError::GDBBusy);
        }

        let command_token = self.new_token();

        command
            .borrow()
            .write_interpreter_string(
                &mut self
                    .process
                    .lock()
                    .await
                    .stdin
                    .as_mut()
                    .ok_or_else(|| AppError::GDBError("Failed to get stdin".to_string()))?,
                command_token,
            )
            .await
            .expect("write interpreter command");

        match self.result_output.recv().await {
            Some(record) => match record.token {
                Some(token) => {
                    if token == command_token {
                        Ok(record)
                    } else {
                        Err(AppError::InvalidArgument(format!(
                            "Unexpected command token: {}",
                            token
                        )))
                    }
                }
                None if command.borrow().operation.is_empty() => Ok(record),
                None => Err(AppError::GDBError(format!(
                    "No command token, expecting {}",
                    command_token
                ))),
            },
            None => Err(AppError::GDBError("no result, expecting {}".to_string())),
        }
    }

    pub async fn execute_later<C: std::borrow::Borrow<commands::MiCommand>>(&mut self, command: C) {
        let command_token = self.new_token();
        command
            .borrow()
            .write_interpreter_string(
                &mut self.process.lock().await.stdin.as_mut().unwrap(),
                command_token,
            )
            .await
            .expect("write interpreter command");
        let _ = self.result_output.recv().await;
    }

    pub async fn is_session_active(&mut self) -> AppResult<bool> {
        let res = self.execute(commands::MiCommand::thread_info(None)).await?;
        Ok(!res.results["threads"].is_empty())
    }
}
