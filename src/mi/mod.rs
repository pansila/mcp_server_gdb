pub mod commands;
pub mod output;

use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use tokio::process::{Child, ChildStdin, Command};
use tracing::debug;
use tracing::info;

use crate::error::AppError;
use crate::error::AppResult;

type Token = u64;

#[allow(clippy::upper_case_acronyms)]
pub struct GDB {
    pub process: Child,
    is_running: Arc<AtomicBool>,
    current_command_token: Token,
    binary_path: PathBuf,
    init_options: Vec<OsString>,
}

pub trait OutOfBandRecordSink: std::marker::Send {
    fn send(&self, record: output::OutOfBandRecord);
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
    gdb_path: PathBuf,
    /// Do not read ~/.gdbinit file (--nh)
    opt_nh: bool,
    /// Do not read any .gdbinit files in any directory (--nx)
    opt_nx: bool,
    /// Do not print version number on startup (--quiet)
    opt_quiet: bool,
    /// Change current directory to DIR (--cd=DIR)
    opt_cd: Option<PathBuf>,
    /// Set serial port baud rate used for remote debugging (-b BAUDRATE)
    opt_bps: Option<u32>,
    /// Read symbols from SYMFILE (--symbols=SYMFILE)
    opt_symbol_file: Option<PathBuf>,
    /// Analyze the core dump COREFILE (--core=COREFILE)
    opt_core_file: Option<PathBuf>,
    /// Attach to running process PID (--pid=PID)
    opt_proc_id: Option<u32>,
    /// Execute GDB commands from FILE (--command=FILE)
    opt_command: Option<PathBuf>,
    /// Search for source files in DIR (--directory=DIR)
    opt_source_dir: Option<PathBuf>,
    /// Arguments to be passed to the inferior program (--args)
    opt_args: Vec<OsString>,
    /// The executable file to debug
    opt_program: Option<PathBuf>,
    /// Use TTY for input/output by the program being debugged (--tty=TTY)
    opt_tty: Option<PathBuf>,
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

    pub fn nh(mut self) -> Self {
        self.opt_nh = true;
        self
    }
    pub fn nx(mut self) -> Self {
        self.opt_nx = true;
        self
    }
    pub fn quiet(mut self) -> Self {
        self.opt_quiet = true;
        self
    }
    pub fn cd(mut self, dir: PathBuf) -> Self {
        self.opt_cd = Some(dir);
        self
    }
    pub fn bps(mut self, bps: u32) -> Self {
        self.opt_bps = Some(bps);
        self
    }
    pub fn symbol_file(mut self, file: PathBuf) -> Self {
        self.opt_symbol_file = Some(file);
        self
    }
    pub fn core_file(mut self, file: PathBuf) -> Self {
        self.opt_core_file = Some(file);
        self
    }
    pub fn proc_id(mut self, pid: u32) -> Self {
        self.opt_proc_id = Some(pid);
        self
    }
    pub fn command_file(mut self, command_file: PathBuf) -> Self {
        self.opt_command = Some(command_file);
        self
    }
    pub fn source_dir(mut self, dir: PathBuf) -> Self {
        self.opt_source_dir = Some(dir);
        self
    }
    pub fn args(mut self, args: &[OsString]) -> Self {
        self.opt_args.extend_from_slice(args);
        self
    }
    pub fn program(mut self, program: PathBuf) -> Self {
        self.opt_program = Some(program);
        self
    }
    pub fn tty(mut self, tty: PathBuf) -> Self {
        self.opt_tty = Some(tty);
        self
    }
    pub async fn try_spawn<S>(self, oob_sink: S) -> AppResult<GDB>
    where
        S: OutOfBandRecordSink + 'static,
    {
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
            gdb_args.push(self.opt_program.unwrap().into());
            for arg in self.opt_args {
                gdb_args.push(arg);
            }
        } else if let Some(program) = self.opt_program {
            gdb_args.push(program.into());
        }

        let mut command = Command::new(self.gdb_path.clone());
        command.arg("--interpreter=mi").args(gdb_args);

        debug!("Starting GDB process with command: {:?}", command);

        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::GDBError(format!("Failed to start GDB process: {}", e)))?;

        let is_running = Arc::new(AtomicBool::new(false));
        let gdb = GDB {
            process: child,
            is_running,
            current_command_token: 0,
            binary_path: self.gdb_path,
            init_options,
        };
        Ok(gdb)
    }
}

impl GDB {
    pub fn interrupt_execution(&self) -> Result<(), nix::Error> {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(self.process.id() as i32), signal::SIGINT)
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
    pub fn get_usable_token(&mut self) -> Token {
        self.current_command_token = self.current_command_token.wrapping_add(1);
        self.current_command_token
    }

    pub fn execute<C: std::borrow::Borrow<commands::MiCommand>>(
        &mut self,
        command: C,
    ) -> Result<output::ResultRecord, ExecuteError> {
        if self.is_running() {
            return Err(ExecuteError::Busy);
        }
        let command_token = self.get_usable_token();

        let mut bytes = Vec::new();
        command
            .borrow()
            .write_interpreter_string(&mut bytes, command_token)
            .expect("write interpreter command");

        info!("Writing msg {}", String::from_utf8_lossy(&bytes),);
        command
            .borrow()
            .write_interpreter_string(&mut self.stdin, command_token)
            .expect("write interpreter command");
        loop {
            match self.result_output.recv() {
                Ok(record) => match record.token {
                    Some(token) if token == command_token => return Ok(record),
                    _ => info!(
                        "Record does not match expected token ({}) and will be dropped: {:?}",
                        command_token, record
                    ),
                },
                Err(_) => return Err(ExecuteError::Quit),
            }
        }
    }

    pub fn execute_later<C: std::borrow::Borrow<commands::MiCommand>>(&mut self, command: C) {
        let command_token = self.get_usable_token();
        command
            .borrow()
            .write_interpreter_string(&mut self.stdin, command_token)
            .expect("write interpreter command");
        let _ = self.result_output.recv();
    }

    pub fn is_session_active(&mut self) -> Result<bool, ExecuteError> {
        let res = self.execute(commands::MiCommand::thread_info(None))?;
        Ok(!res.results["threads"].is_empty())
    }
}
