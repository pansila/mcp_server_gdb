use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::TRANSPORT;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::mi::commands::{BreakPointLocation, BreakPointNumber, MiCommand, RegisterFormat};
use crate::mi::output::{OutOfBandRecord, ResultClass, ResultRecord};
use crate::mi::{GDB, GDBBuilder};
use crate::models::{BreakPoint, GDBSession, GDBSessionStatus, Register, StackFrame, Variable};

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
    /// OOB handle
    oob_handle: JoinHandle<()>,
}

impl GDBManager {
    /// Create a new GDB manager
    pub fn new() -> Self {
        Self { config: Config::default(), sessions: Mutex::new(HashMap::new()) }
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

        let (oob_src, mut oob_sink) = mpsc::channel(100);
        let gdb = gdb_builder.try_spawn(oob_src)?;

        let oob_handle = tokio::spawn(async move {
            loop {
                match oob_sink.recv().await {
                    Some(record) => match record {
                        OutOfBandRecord::AsyncRecord { results, .. } => {
                            let transport = TRANSPORT.lock().await;
                            if let Some(transport) = transport.as_ref() {
                                if let Err(e) = transport
                                    .send_notification("create_session", Some(results))
                                    .await
                                {
                                    error!("Failed to send ping to session: {:?}", e);
                                }
                            } else {
                                warn!("Sink Channel closed");
                                break;
                            }
                        }
                        OutOfBandRecord::StreamRecord { data, .. } => {
                            debug!("StreamRecord: {:?}", data);
                        }
                    },
                    None => {
                        debug!("Source Channel closed");
                        break;
                    }
                }
            }
        });

        // Create session information
        let session = GDBSession {
            id: session_id.clone(),
            status: GDBSessionStatus::Created,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        };

        // Store session
        let handle = GDBSessionHandle { info: session, gdb, oob_handle };

        self.sessions.lock().await.insert(session_id.clone(), handle);

        // Send empty command to GDB to flush the welcome messages
        let _ = self.send_command(&session_id, &MiCommand::empty()).await?;

        Ok(session_id)
    }

    /// Get all sessions
    pub async fn get_all_sessions(&self) -> AppResult<Vec<GDBSession>> {
        let sessions = self.sessions.lock().await;
        let result = sessions.values().map(|handle| handle.info.clone()).collect();
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
        let _ = match self.send_command_with_timeout(session_id, &MiCommand::exit()).await {
            Ok(result) => Some(result),
            Err(e) => {
                warn!("GDB exit command timed out, forcing process termination: {}", e.to_string());
                // Ignore timeout error, continue to force terminate the process
                None
            }
        };

        let mut sessions = self.sessions.lock().await;
        let handle = sessions.remove(session_id);

        if let Some(handle) = handle {
            handle.oob_handle.abort();
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
        let output = record.results.to_string();

        debug!("GDB output: {}", output);
        Ok(record)
    }

    /// Send GDB command with timeout
    async fn send_command_with_timeout(
        &self,
        session_id: &str,
        command: &MiCommand,
    ) -> AppResult<ResultRecord> {
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
        let response = self.send_command_with_timeout(session_id, &MiCommand::exec_run()).await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response.results.to_string())
    }

    /// Stop debugging
    pub async fn stop_debugging(&self, session_id: &str) -> AppResult<String> {
        let response =
            self.send_command_with_timeout(session_id, &MiCommand::exec_interrupt()).await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Stopped;
        }

        Ok(response.results.to_string())
    }

    /// Get breakpoint list
    pub async fn get_breakpoints(&self, session_id: &str) -> AppResult<Vec<BreakPoint>> {
        let response =
            self.send_command_with_timeout(session_id, &MiCommand::breakpoints_list()).await?;

        let table = response
            .results
            .get("BreakpointTable")
            .ok_or(AppError::NotFound("BreakpointTable not found".to_string()))?;
        let body = table.get("body").ok_or(AppError::NotFound("body not found".to_string()))?;
        Ok(serde_json::from_value(body.to_owned())?)
    }

    /// Set breakpoint
    pub async fn set_breakpoint(
        &self,
        session_id: &str,
        file: &Path,
        line: usize,
    ) -> AppResult<BreakPoint> {
        let command = MiCommand::insert_breakpoint(BreakPointLocation::Line(file, line));
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(serde_json::from_value(
            response
                .results
                .get("bkpt")
                .ok_or(AppError::NotFound("bkpt not found in the result".to_string()))?
                .to_owned(),
        )?)
    }

    /// Delete breakpoint
    pub async fn delete_breakpoint(
        &self,
        session_id: &str,
        breakpoints: Vec<String>,
    ) -> AppResult<()> {
        let command = MiCommand::delete_breakpoints(
            breakpoints
                .iter()
                .map(|num| serde_json::from_str::<BreakPointNumber>(num))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let response = self.send_command_with_timeout(session_id, &command).await?;
        if response.class != ResultClass::Done {
            return Err(AppError::GDBError(response.results.to_string()));
        }

        Ok(())
    }

    /// Get stack frames
    pub async fn get_stack_frames(&self, session_id: &str) -> AppResult<Vec<StackFrame>> {
        let command = MiCommand::stack_list_frames(None, None);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(serde_json::from_value(
            response
                .results
                .get("stack")
                .ok_or(AppError::NotFound("stack not found".to_string()))?
                .to_owned(),
        )?)
    }

    /// Get local variables
    pub async fn get_local_variables(
        &self,
        session_id: &str,
        frame_id: Option<usize>,
    ) -> AppResult<Vec<Variable>> {
        let command = MiCommand::stack_list_variables(None, frame_id, None);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(serde_json::from_value(
            response
                .results
                .get("variables")
                .ok_or(AppError::NotFound("expect variables in result".to_string()))?
                .to_owned(),
        )?)
    }

    /// Get registers
    pub async fn get_registers(
        &self,
        session_id: &str,
        reg_list: Option<Vec<String>>,
    ) -> AppResult<Vec<Register>> {
        let reg_list = reg_list
            .map(|s| s.iter().map(|num| num.parse::<usize>()).collect::<Result<Vec<_>, _>>())
            .transpose()?;
        let command = MiCommand::data_list_register_names(reg_list.clone());
        let response = self.send_command_with_timeout(session_id, &command).await?;
        let names: Vec<String> = serde_json::from_value(
            response
                .results
                .get("register-names")
                .ok_or(AppError::NotFound("register-names not found".to_string()))?
                .to_owned(),
        )?;

        let command = MiCommand::data_list_register_values(RegisterFormat::Hex, reg_list);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        let registers: Vec<Register> = serde_json::from_value(
            response
                .results
                .get("register-values")
                .ok_or(AppError::NotFound("expect register-values".to_string()))?
                .to_owned(),
        )?;
        Ok(registers
            .into_iter()
            .map(|mut r| {
                r.name = names.get(r.number).cloned();
                r
            })
            .collect::<_>())
    }

    /// Get register names
    pub async fn get_register_names(
        &self,
        session_id: &str,
        reg_list: Option<Vec<String>>,
    ) -> AppResult<Vec<Register>> {
        let reg_list = reg_list
            .map(|s| s.iter().map(|num| num.parse::<usize>()).collect::<Result<Vec<_>, _>>())
            .transpose()?;
        let command = MiCommand::data_list_register_names(reg_list);
        let response = self.send_command_with_timeout(session_id, &command).await?;

        Ok(serde_json::from_value(
            response
                .results
                .get("register-values")
                .ok_or(AppError::NotFound("expect register-values".to_string()))?
                .to_owned(),
        )?)
    }

    /// Continue execution
    pub async fn continue_execution(&self, session_id: &str) -> AppResult<String> {
        let response =
            self.send_command_with_timeout(session_id, &MiCommand::exec_continue()).await?;

        // Update session status
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.get_mut(session_id) {
            handle.info.status = GDBSessionStatus::Running;
        }

        Ok(response.results.to_string())
    }

    /// Step execution
    pub async fn step_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self.send_command_with_timeout(session_id, &MiCommand::exec_step()).await?;

        Ok(response.results.to_string())
    }

    /// Next execution
    pub async fn next_execution(&self, session_id: &str) -> AppResult<String> {
        let response = self.send_command_with_timeout(session_id, &MiCommand::exec_next()).await?;

        Ok(response.results.to_string())
    }
}
