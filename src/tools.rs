use crate::gdb::GDBManager;
use anyhow::Result;
use mcp_core::{tool_text_content, types::ToolResponseContent};
use mcp_core_macros::tool;
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

static GDB_MANAGER: LazyLock<Arc<GDBManager>> = LazyLock::new(|| Arc::new(GDBManager::new()));

pub fn init_gdb_manager() {
    LazyLock::force(&GDB_MANAGER);
}

#[tool(
    name = "create_session",
    description = "Create a new GDB debugging session with optional parameters,\
                   returns a session ID (UUID) if successful",
    params(
        program = "if provided, path to the executable to debug",
        nh = "if provided, do not read ~/.gdbinit file",
        nx = "if provided, do not read any .gdbinit files in any directory",
        quiet = "if provided, do not print version number on startup",
        cd = "if provided, change current directory to DIR",
        bps = "if provided, set serial port baud rate used for remote debugging",
        symbol_file = "if provided, read symbols from SYMFILE",
        core_file = "if provided, analyze the core dump COREFILE",
        proc_id = "if provided, attach to running process PID",
        command = "if provided, execute GDB commands from FILE",
        source_dir = "if provided, search for source files in DIR",
        args = "if provided, arguments to be passed to the inferior program",
        tty = "if provided, use TTY for input/output by the program being debugged",
        gdb_path = "if provided, path to the GDB executable",
    )
)]
pub async fn create_session_tool(
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
) -> Result<ToolResponseContent> {
    let session = GDB_MANAGER
        .create_session(
            program,
            nh,
            nx,
            quiet,
            cd,
            bps,
            symbol_file,
            core_file,
            proc_id,
            command,
            source_dir,
            args,
            tty,
            gdb_path,
        )
        .await?;
    Ok(tool_text_content!(format!("Created GDB session: {}", session)))
}

#[tool(
    name = "get_session",
    description = "Get a GDB debugging session by ID",
    params(session_id = "The ID of the GDB session")
)]
pub async fn get_session_tool(session_id: String) -> Result<ToolResponseContent> {
    let session = GDB_MANAGER.get_session(&session_id).await?;
    Ok(tool_text_content!(format!("Session: {:?}", session)))
}

#[tool(name = "get_all_sessions", description = "Get all GDB debugging sessions", params())]
pub async fn get_all_sessions_tool() -> Result<ToolResponseContent> {
    let sessions = GDB_MANAGER.get_all_sessions().await?;
    Ok(tool_text_content!(format!("Sessions: {:?}", sessions)))
}

#[tool(
    name = "close_session",
    description = "Close a GDB debugging session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn close_session_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.close_session(&session_id).await?;
    Ok(tool_text_content!("Closed GDB session".to_string()))
}

#[tool(
    name = "start_debugging",
    description = "Start debugging in a session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn start_debugging_tool(session_id: String) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.start_debugging(&session_id).await?;
    Ok(tool_text_content!(format!("Started debugging: {}", ret)))
}

#[tool(
    name = "stop_debugging",
    description = "Stop debugging in a session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn stop_debugging_tool(session_id: String) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.stop_debugging(&session_id).await?;
    Ok(tool_text_content!(format!("Stopped debugging: {}", ret)))
}

#[tool(
    name = "get_breakpoints",
    description = "Get all breakpoints in the current GDB session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn get_breakpoints_tool(session_id: String) -> Result<ToolResponseContent> {
    let breakpoints = GDB_MANAGER.get_breakpoints(&session_id).await?;
    Ok(tool_text_content!(format!("Breakpoints: {:?}", breakpoints)))
}

#[tool(
    name = "set_breakpoint",
    description = "Set a breakpoint in the code",
    params(
        session_id = "The ID of the GDB session",
        file = "Source file path",
        line = "Line number"
    )
)]
pub async fn set_breakpoint_tool(
    session_id: String,
    file: String,
    line: usize,
) -> Result<ToolResponseContent> {
    let breakpoint = GDB_MANAGER.set_breakpoint(&session_id, &PathBuf::from(file), line).await?;
    Ok(tool_text_content!(format!("Set breakpoint: {:?}", breakpoint)))
}

#[tool(
    name = "delete_breakpoint",
    description = "Delete one or more breakpoints in the code",
    params(
        session_id = "The ID of the GDB session",
        breakpoints = "The list of the breakpoint numbers, separated by commas"
    )
)]
pub async fn delete_breakpoint_tool(
    session_id: String,
    breakpoints: String,
) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.delete_breakpoint(&session_id, &breakpoints).await?;
    Ok(tool_text_content!(format!("Deleted breakpoint: {}", ret)))
}

#[tool(
    name = "get_stack_frames",
    description = "Get stack frames in the current GDB session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn get_stack_frames_tool(session_id: String) -> Result<ToolResponseContent> {
    let frames = GDB_MANAGER.get_stack_frames(&session_id).await?;
    Ok(tool_text_content!(format!("Stack frames: {:?}", frames)))
}

#[tool(
    name = "get_local_variables",
    description = "Get local variables in the current stack frame",
    params(session_id = "The ID of the GDB session", frame_id = "The ID of the stack frame")
)]
pub async fn get_local_variables_tool(
    session_id: String,
    frame_id: usize,
) -> Result<ToolResponseContent> {
    let variables = GDB_MANAGER.get_local_variables(&session_id, frame_id).await?;
    Ok(tool_text_content!(format!("Local variables: {:?}", variables)))
}

#[tool(
    name = "continue_execution",
    description = "Continue program execution",
    params(session_id = "The ID of the GDB session")
)]
pub async fn continue_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.continue_execution(&session_id).await?;
    Ok(tool_text_content!(format!("Continued execution: {}", ret)))
}

#[tool(
    name = "step_execution",
    description = "Step into next line",
    params(session_id = "The ID of the GDB session")
)]
pub async fn step_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.step_execution(&session_id).await?;
    Ok(tool_text_content!(format!("Stepped into next line: {}", ret)))
}

#[tool(
    name = "next_execution",
    description = "Step over next line",
    params(session_id = "The ID of the GDB session")
)]
pub async fn next_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    let ret = GDB_MANAGER.next_execution(&session_id).await?;
    Ok(tool_text_content!(format!("Stepped over next line: {}", ret)))
}
