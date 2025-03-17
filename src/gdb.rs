use regex::Regex;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TokioBufReader},
    process::{Child, Command},
};
use tracing::debug;
use uuid::Uuid;

use crate::mi::{
    self, ExecuteError, GDB, GDBBuilder, OutOfBandRecordSink,
    commands::{BreakPointLocation, BreakPointNumber, MiCommand},
    output::{BreakPointEvent, ResultClass},
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    models::{Breakpoint, GDBSession, GDBSessionStatus, StackFrame, Variable},
};
use json::JsonValue;
use json::object::Object;

/// GDB Session Manager
pub struct GDBManager {
    /// Configuration
    config: Config,
    /// Session mapping table
    sessions: RwLock<HashMap<String, GDBSessionHandle>>,
}

/// GDB Session Handle
struct GDBSessionHandle {
    /// Session information
    info: GDBSession,
    /// GDB process
    // process: Arc<Mutex<Child>>,
    gdb: GDB,
}

impl GDBManager {
    /// Create a new GDB manager
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new GDB session
    pub async fn create_session(
        &self,
        program: Option<PathBuf>,
        nh: Option<bool>,
        nx: Option<bool>,
        quiet: Option<bool>,
        cd: Option<PathBuf>,
        bps: Option<u32>,
        symbol_file: Option<PathBuf>,
        core_file: Option<PathBuf>,
        proc_id: Option<u32>,
        command: Option<PathBuf>,
        source_dir: Option<PathBuf>,
        args: Option<Vec<OsString>>,
        tty: Option<PathBuf>,
    ) -> AppResult<String> {
        // Generate unique session ID
        let session_id = Uuid::new_v4().to_string();

        let mut gdb_builder = GDBBuilder::new(self.config.gdb_path.clone());
        if let Some(nh) = nh {
            if nh {
                gdb_builder = gdb_builder.nh();
            }
        }
        if let Some(nx) = nx {
            if nx {
                gdb_builder = gdb_builder.nx();
            }
        }
        if let Some(quiet) = quiet {
            if quiet {
                gdb_builder = gdb_builder.quiet();
            }
        }
        if let Some(cd) = cd {
            gdb_builder = gdb_builder.cd(cd);
        }
        if let Some(bps) = bps {
            gdb_builder = gdb_builder.bps(bps);
        }
        if let Some(symbol_file) = symbol_file {
            gdb_builder = gdb_builder.symbol_file(symbol_file);
        }
        if let Some(core_file) = core_file {
            gdb_builder = gdb_builder.core_file(core_file);
        }
        if let Some(proc_id) = proc_id {
            gdb_builder = gdb_builder.proc_id(proc_id);
        }
        if let Some(program) = program {
            gdb_builder = gdb_builder.program(program);
        }
        if let Some(command) = command {
            gdb_builder = gdb_builder.command_file(command);
        }
        if let Some(source_dir) = source_dir {
            gdb_builder = gdb_builder.source_dir(source_dir);
        }
        if let Some(args) = args {
            gdb_builder = gdb_builder.args(&args);
        }
        if let Some(tty) = tty {
            gdb_builder = gdb_builder.tty(tty);
        }

        let gdb = gdb_builder.try_spawn(oob_sink).await?;

        // Create session information
        let session = GDBSession {
            id: session_id.clone(),
            status: GDBSessionStatus::Created,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        // Store session
        let handle = GDBSessionHandle { info: session, gdb };

        self.sessions
            .write()
            .await
            .insert(session_id.clone(), handle);

        // Send empty command to GDB to flush the welcome messages
        let _ = self.send_command(&session_id, "").await?;

        Ok(session_id)
    }

    /// Get all sessions
    pub async fn get_all_sessions(&self) -> AppResult<Vec<GDBSession>> {
        let sessions = self.sessions.read().await;
        let result = sessions
            .values()
            .map(|handle| handle.info.clone())
            .collect();
        Ok(result)
    }

    /// Get specific session
    pub async fn get_session(&self, session_id: &str) -> AppResult<GDBSession> {
        let sessions = self.sessions.read().await;
        let handle = sessions
            .get(session_id)
            .ok_or_else(|| AppError::NotFound(format!("Session {} does not exist", session_id)))?;
        Ok(handle.info.clone())
    }

    /// Close session
    pub async fn close_session(&self, session_id: &str) -> AppResult<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(handle) = sessions.remove(session_id) {
            // Use timeout when sending exit command
            let command_timeout = self.config.command_timeout;
            let _ = match tokio::time::timeout(
                Duration::from_secs(command_timeout),
                self.send_raw_command(&handle, "-gdb-exit"),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    debug!("GDB exit command timed out, forcing process termination");
                    Ok(String::new()) // Ignore timeout error, continue to force terminate the process
                }
            };

            // Terminate process
            let mut process = handle.process.lock().await;
            let _ = process.kill().await; // Ignore possible errors, process may have already terminated
        }

        Ok(())
    }

    /// Send GDB command
    pub async fn send_command(&self, session_id: &str, command: &str) -> AppResult<String> {
        let sessions = self.sessions.read().await;
        let handle = sessions
            .get(session_id)
            .ok_or_else(|| AppError::NotFound(format!("Session {} does not exist", session_id)))?;

        let output = self.send_raw_command(handle, command).await?;

        // Parse output
        let success = !output.contains("^error");
        if !success {
            // Extract error message
            static ERROR_REGEX: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r#"\^error,msg="(.+)""#).unwrap());

            let error = ERROR_REGEX
                .captures(&output)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(AppError::GDBError(error));
        }

        Ok(output)
    }

    /// Send raw command to GDB
    async fn send_raw_command(
        &self,
        handle: &GDBSessionHandle,
        command: &str,
    ) -> AppResult<String> {
        let command = format!("{}\n", command);
        debug!("Sending raw command: {}", command);

        // Send command
        {
            let mut process = handle.process.lock().await;
            let stdin = process
                .stdin
                .as_mut()
                .ok_or_else(|| AppError::GDBError("Cannot access GDB stdin".to_string()))?;

            stdin
                .write_all(command.as_bytes())
                .await
                .map_err(|e| AppError::GDBError(format!("Failed to send command: {}", e)))?;
        } // Lock is released here

        // Read response
        let mut output = String::new();
        {
            let mut process = handle.process.lock().await;
            let stdout = process
                .stdout
                .as_mut()
                .ok_or_else(|| AppError::GDBError("Cannot access GDB stdout".to_string()))?;

            let mut reader = TokioBufReader::new(stdout);
            let mut line_count = 0;
            let mut buffer = String::new();

            while reader
                .read_line(&mut buffer)
                .await
                .map_err(|e| AppError::GDBError(format!("Failed to read GDB output: {}", e)))?
                > 0
            {
                output.push_str(&buffer);

                // Check if command completion marker
                if buffer.starts_with("^done")
                    || buffer.starts_with("^error")
                    || buffer.starts_with("^exit")
                {
                    buffer.clear();
                    break;
                }

                // Safety limit to prevent infinite loop
                line_count += 1;
                if line_count > 1000 {
                    break;
                }

                buffer.clear();
            }
        } // Lock is released here

        debug!("GDB output: {}", output);
        Ok(output)
    }

    /// Send GDB command with timeout
    async fn send_command_with_timeout(
        &self,
        session_id: &str,
        command: &str,
    ) -> AppResult<String> {
        let command_timeout = self.config.command_timeout;
        match tokio::time::timeout(
            Duration::from_secs(command_timeout),
            self.send_command(session_id, command),
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AppError::GDBTimeout),
        }
    }

    /// Start debugging
    pub async fn start_debugging(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, "-exec-run")
            .await?;

        // Update session status
        let mut sessions = self.sessions.write().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response)
    }

    /// Stop debugging
    pub async fn stop_debugging(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, "-exec-interrupt")
            .await?;

        // Update session status
        let mut sessions = self.sessions.write().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Stopped;
        }

        Ok(response)
    }

    /// Get breakpoint list
    pub async fn get_breakpoints(&self, session_id: &str) -> AppResult<Vec<Breakpoint>> {
        let response = self
            .send_command_with_timeout(session_id, "-break-list")
            .await?;

        // Parse breakpoint information (simplified version, actually needs more complex parsing)
        let breakpoints = Vec::new();

        // There should be more complex parsing logic here, this is just a simplified example
        // Actual implementation needs to parse correctly according to GDB/MI output format

        Ok(breakpoints)
    }

    /// Set breakpoint
    pub async fn set_breakpoint(
        &self,
        session_id: &str,
        file: &str,
        line: u32,
    ) -> AppResult<Breakpoint> {
        let command = format!("-break-insert {}:{}", file, line);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        // Parse breakpoint ID (simplified)
        let breakpoint_id = Uuid::new_v4().to_string(); // Should actually be extracted from response

        Ok(Breakpoint {
            id: breakpoint_id,
            file: file.to_string(),
            line,
            enabled: true,
        })
    }

    /// Delete breakpoint
    pub async fn delete_breakpoint(
        &self,
        session_id: &str,
        breakpoint_id: &str,
    ) -> AppResult<String> {
        let command = format!("-break-delete {}", breakpoint_id);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(response)
    }

    /// Get stack frames
    pub async fn get_stack_frames(&self, session_id: &str) -> AppResult<Vec<StackFrame>> {
        let response = self
            .send_command_with_timeout(session_id, "-stack-list-frames")
            .await?;

        // Parse stack frame information (simplified)
        let frames = Vec::new(); // Actually needs to parse response

        Ok(frames)
    }

    /// Get local variables
    pub async fn get_local_variables(
        &self,
        session_id: &str,
        frame_id: u32,
    ) -> AppResult<Vec<Variable>> {
        let command = format!("-stack-list-variables --frame {} --simple-values", frame_id);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        // Parse variable information (simplified)
        let variables = Vec::new(); // Actually needs to parse response

        Ok(variables)
    }

    /// Continue execution
    pub async fn continue_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, "-exec-continue")
            .await?;

        // Update session status
        let mut sessions = self.sessions.write().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response)
    }

    /// Step execution
    pub async fn step_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, "-exec-step")
            .await?;

        Ok(response)
    }

    /// Next execution
    pub async fn next_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, "-exec-next")
            .await?;

        Ok(response)
    }
}
