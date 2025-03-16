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
    name = "get_session",
    description = "Get a GDB debugging session by ID",
    params(session_id = "The ID of the GDB session")
)]
pub async fn get_session_tool(session_id: String) -> Result<ToolResponseContent> {
    let session = GDB_MANAGER.get_session(&session_id).await?;
    Ok(tool_text_content!(format!("Session: {:?}", session)))
}

#[tool(
    name = "get_all_sessions",
    description = "Get all GDB debugging sessions",
    params()
)]
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
    name = "get_breakpoints",
    description = "Get all breakpoints in the current GDB session",
    params(session_id = "The ID of the GDB session")
)]
pub async fn get_breakpoints_tool(session_id: String) -> Result<ToolResponseContent> {
    let breakpoints = GDB_MANAGER.get_breakpoints(&session_id).await?;
    Ok(tool_text_content!(format!(
        "Breakpoints: {:?}",
        breakpoints
    )))
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
    name = "delete_breakpoint",
    description = "Delete a breakpoint in the code",
    params(
        session_id = "The ID of the GDB session",
        breakpoint_id = "The ID of the breakpoint"
    )
)]
pub async fn delete_breakpoint_tool(
    session_id: String,
    breakpoint_id: String,
) -> Result<ToolResponseContent> {
    GDB_MANAGER
        .delete_breakpoint(&session_id, &breakpoint_id)
        .await?;
    Ok(tool_text_content!("Deleted breakpoint".to_string()))
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
    params(
        session_id = "The ID of the GDB session",
        frame_id = "The ID of the stack frame"
    )
)]
pub async fn get_local_variables_tool(
    session_id: String,
    frame_id: u32,
) -> Result<ToolResponseContent> {
    let variables = GDB_MANAGER
        .get_local_variables(&session_id, frame_id)
        .await?;
    Ok(tool_text_content!(format!(
        "Local variables: {:?}",
        variables
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use clap::{Parser, ValueEnum};
    use mcp_core::{
        client::ClientBuilder,
        protocol::RequestOptions,
        transport::{ClientSseTransportBuilder, ClientStdioTransport},
        types::{ClientCapabilities, Implementation},
    };
    use serde_json::json;
    use std::time::Duration;

    #[tokio::test]
    async fn test_echo_client() -> Result<()> {
        let transport = ClientStdioTransport::new("./target/debug/echo_server", &[])?;
        let client = ClientBuilder::new(transport.clone()).build();
        tokio::time::sleep(Duration::from_millis(100)).await;
        client.open().await?;

        client
            .initialize(
                Implementation {
                    name: "echo".to_string(),
                    version: "1.0".to_string(),
                },
                ClientCapabilities::default(),
            )
            .await?;

        client
            .call_tool(
                "echo",
                Some(json!({
                    "message": "Hello, world!"
                })),
            )
            .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_create_session_tool() -> Result<()> {
        let response = create_session_tool(None).await?;
        // 只检查是否成功返回，不检查具体内容
        assert!(format!("{:?}", response).contains("Created GDB session"));
        Ok(())
    }
}
