use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::mi::{
    self, ExecuteError, GDB, GDBBuilder,
    commands::{BreakPointLocation, BreakPointNumber, MiCommand},
    output::{BreakPointEvent, ResultClass, ResultRecord},
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    models::{Breakpoint, GDBSession, GDBSessionStatus, StackFrame, Variable},
};

/// GDB Session Manager
pub struct GDBManager {
    /// Configuration
    config: Config,
    /// Session mapping table
    sessions: Mutex<HashMap<String, GDBSessionHandle>>,
}

/// GDB Session Handle
struct GDBSessionHandle {
    /// Session information
    info: GDBSession,
    /// GDB instance
    gdb: GDB,
}

impl GDBManager {
    /// Create a new GDB manager
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            sessions: Mutex::new(HashMap::new()),
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
        gdb_path: Option<PathBuf>,
    ) -> AppResult<String> {
        // Generate unique session ID
        let session_id = Uuid::new_v4().to_string();

        let gdb_builder = GDBBuilder {
            gdb_path: gdb_path.unwrap_or_else(|| PathBuf::from("gdb")),
            opt_nh: nh.unwrap_or(false),
            opt_nx: nx.unwrap_or(false),
            opt_quiet: quiet.unwrap_or(false),
            opt_cd: cd,
            opt_bps: bps,
            opt_symbol_file: symbol_file,
            opt_core_file: core_file,
            opt_proc_id: proc_id,
            opt_command: command,
            opt_source_dir: source_dir,
            opt_args: args.unwrap_or(vec![]),
            opt_program: program,
            opt_tty: tty,
        };

        // TODO: connect it to MCP notification
        let (oob_src, oob_sink) = mpsc::channel(100);
        let gdb = gdb_builder.try_spawn(oob_src)?;

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
            .lock()
            .await
            .insert(session_id.clone(), handle);

        // Send empty command to GDB to flush the welcome messages
        let _ = self.send_command(&session_id, &MiCommand::empty()).await?;

        Ok(session_id)
    }

    /// Get all sessions
    pub async fn get_all_sessions(&self) -> AppResult<Vec<GDBSession>> {
        let sessions = self.sessions.lock().await;
        let result = sessions
            .values()
            .map(|handle| handle.info.clone())
            .collect();
        Ok(result)
    }

    /// Get specific session
    pub async fn get_session(&self, session_id: &str) -> AppResult<GDBSession> {
        let sessions = self.sessions.lock().await;
        let handle = sessions
            .get(session_id)
            .ok_or_else(|| AppError::NotFound(format!("Session {} does not exist", session_id)))?;
        Ok(handle.info.clone())
    }

    /// Close session
    pub async fn close_session(&self, session_id: &str) -> AppResult<()> {
        let mut sessions = self.sessions.lock().await;

        if let Some(handle) = sessions.remove(session_id) {
            let _ = match self
                .send_command_with_timeout(session_id, &MiCommand::exit())
                .await
            {
                Ok(result) => Some(result),
                Err(_) => {
                    warn!("GDB exit command timed out, forcing process termination");
                    // Ignore timeout error, continue to force terminate the process
                    None
                }
            };

            // Terminate process
            let mut process = handle.gdb.process.lock().await;
            let _ = process.kill().await; // Ignore possible errors, process may have already terminated
        }

        Ok(())
    }

    /// Send GDB command
    pub async fn send_command(
        &self,
        session_id: &str,
        command: &MiCommand,
    ) -> AppResult<ResultRecord> {
        let mut sessions = self.sessions.lock().await;
        let handle = sessions
            .get_mut(session_id)
            .ok_or_else(|| AppError::NotFound(format!("Session {} does not exist", session_id)))?;

        let record = handle.gdb.execute(command).await?;
        let output = record.results.dump();

        debug!("GDB output: {}", output);
        Ok(record)
    }

    /// Send GDB command with timeout
    async fn send_command_with_timeout(
        &self,
        session_id: &str,
        command: &MiCommand,
    ) -> AppResult<String> {
        let command_timeout = self.config.command_timeout;
        match tokio::time::timeout(
            Duration::from_secs(command_timeout),
            self.send_command(session_id, command),
        )
        .await
        {
            Ok(Ok(result)) => Ok(result.results.dump()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AppError::GDBTimeout),
        }
    }

    /// Start debugging
    pub async fn start_debugging(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::exec_run())
            .await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response)
    }

    /// Stop debugging
    pub async fn stop_debugging(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::exec_interrupt())
            .await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Stopped;
        }

        Ok(response)
    }

    /// Get breakpoint list
    pub async fn get_breakpoints(&self, session_id: &str) -> AppResult<Vec<Breakpoint>> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::breakpoints_list())
            .await?;

        // TODO: parse breakpoint table to a MD table

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
        file: &Path,
        line: usize,
    ) -> AppResult<Breakpoint> {
        let command = MiCommand::insert_breakpoint(BreakPointLocation::Line(file, line));
        let response = self.send_command_with_timeout(session_id, &command).await?;

        // Parse breakpoint ID (simplified)
        let breakpoint_id = Uuid::new_v4().to_string(); // Should actually be extracted from response

        Ok(Breakpoint {
            id: breakpoint_id,
            file: file.to_string_lossy().to_string(),
            line,
            enabled: true,
        })
    }

    /// Delete breakpoint
    pub async fn delete_breakpoint(
        &self,
        session_id: &str,
        breakpoints: &str,
    ) -> AppResult<String> {
        let command = MiCommand::delete_breakpoints(
            breakpoints
                .split(',')
                .map(|num| num.to_string().into())
                .collect(),
        );
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(response)
    }

    /// Get stack frames
    pub async fn get_stack_frames(&self, session_id: &str) -> AppResult<Vec<StackFrame>> {
        let command = MiCommand::stack_list_frames(None, None);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        // Parse stack frame information (simplified)
        let frames = Vec::new(); // Actually needs to parse response

        Ok(frames)
    }

    /// Get local variables
    pub async fn get_local_variables(
        &self,
        session_id: &str,
        frame_id: usize,
    ) -> AppResult<Vec<Variable>> {
        let command = MiCommand::stack_list_variables(None, Some(frame_id));
        let response = self.send_command_with_timeout(session_id, &command).await?;

        // Parse variable information (simplified)
        let variables = Vec::new(); // Actually needs to parse response

        Ok(variables)
    }

    /// Continue execution
    pub async fn continue_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::exec_continue())
            .await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response)
    }

    /// Step execution
    pub async fn step_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::exec_step())
            .await?;

        Ok(response)
    }

    /// Next execution
    pub async fn next_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self
            .send_command_with_timeout(session_id, &MiCommand::exec_next())
            .await?;

        Ok(response)
    }
}
