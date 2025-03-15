use crate::gdb::GDBManager;
use anyhow::Result;
use mcp_core::{tool_text_content, types::ToolResponseContent};
use mcp_core_macros::tool;
use std::sync::{Arc, LazyLock};

static GDB_MANAGER: LazyLock<Arc<GDBManager>> = LazyLock::new(|| Arc::new(GDBManager::new()));

pub fn init_gdb_manager() {
    LazyLock::force(&GDB_MANAGER);
}

#[tool(
    name = "create_session",
    description = "Create a new GDB debugging session",
    params(executable_path = "Optional path to the executable to debug")
)]
pub async fn create_session_tool(executable_path: Option<String>) -> Result<ToolResponseContent> {
    let session = GDB_MANAGER.create_session(executable_path).await?;
    Ok(tool_text_content!(format!(
        "Created GDB session: {}",
        session.id
    )))
}

#[tool(
    name = "start_debugging",
    description = "Start debugging in a session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn start_debugging_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.start_debugging(&session_id).await?;
    Ok(tool_text_content!("Started debugging".to_string()))
}

#[tool(
    name = "stop_debugging",
    description = "Stop debugging in a session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn stop_debugging_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.stop_debugging(&session_id).await?;
    Ok(tool_text_content!("Stopped debugging".to_string()))
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
    line: u32,
) -> Result<ToolResponseContent> {
    let breakpoint = GDB_MANAGER.set_breakpoint(&session_id, &file, line).await?;
    Ok(tool_text_content!(format!(
        "Set breakpoint: {:?}",
        breakpoint
    )))
}

#[tool(
    name = "continue_execution",
    description = "Continue program execution",
    params(session_id = "The ID of the GDB session")
)]
pub async fn continue_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.continue_execution(&session_id).await?;
    Ok(tool_text_content!("Continued execution".to_string()))
}

#[tool(
    name = "step_execution",
    description = "Step into next line",
    params(session_id = "The ID of the GDB session")
)]
pub async fn step_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.step_execution(&session_id).await?;
    Ok(tool_text_content!("Stepped into next line".to_string()))
}

#[tool(
    name = "next_execution",
    description = "Step over next line",
    params(session_id = "The ID of the GDB session")
)]
pub async fn next_execution_tool(session_id: String) -> Result<ToolResponseContent> {
    GDB_MANAGER.next_execution(&session_id).await?;
    Ok(tool_text_content!("Stepped over next line".to_string()))
}
