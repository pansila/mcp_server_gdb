use serde::{Deserialize, Serialize};

/// GDB session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDBSession {
    /// Session ID
    pub id: String,
    /// Session status
    pub status: GDBSessionStatus,
    /// Path of the file being debugged
    pub file_path: Option<String>,
    /// Creation time
    pub created_at: u64,
}

/// GDB session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GDBSessionStatus {
    /// Created but not started
    Created,
    /// Running
    Running,
    /// Program stopped at breakpoint
    Stopped,
    /// Session terminated
    Terminated,
}

/// GDB command request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDBCommandRequest {
    /// GDB/MI command
    pub command: String,
}

/// GDB command response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDBCommandResponse {
    /// Success status
    pub success: bool,
    /// Response content
    pub output: String,
    /// Error message (if any)
    pub error: Option<String>,
}

/// Create session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Executable file path (optional)
    pub executable_path: Option<String>,
}

/// Breakpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Breakpoint ID
    pub id: String,
    /// File path
    pub file: String,
    /// Line number
    pub line: u32,
    /// Enabled status
    pub enabled: bool,
}

/// Stack frame information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame level
    pub level: u32,
    /// Function name
    pub function: String,
    /// File name
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Address
    pub address: String,
}

/// Variable information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable value
    pub value: String,
    /// Variable type
    pub type_name: Option<String>,
}
