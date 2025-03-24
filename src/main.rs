mod config;
mod error;
mod gdb;
mod mi;
mod models;
mod tools;

use std::sync::{Arc, LazyLock};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use mcp_core::{
    server::{Server, ServerProtocolBuilder},
    transport::{ServerSseTransport, ServerStdioTransport, Transport},
    types::ServerCapabilities,
};
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{debug, info};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
enum TransportType {
    Stdio,
    Sse,
}

pub static TRANSPORT: LazyLock<Mutex<Option<Arc<Box<dyn Transport>>>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Transport type to use
    #[arg(value_enum, default_value_t = TransportType::Stdio)]
    transport: TransportType,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args = Args::parse();

    let file_appender = RollingFileAppender::new(Rotation::DAILY, "logs", "mcp-gdb.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Initialize logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::try_new(&args.log_level).unwrap_or_else(|_| EnvFilter::new("info"))
        }))
        // needs to go to file due to stdio transport
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .init();

    // Get configuration
    let config = config::Config::default();
    debug!("GDB path: {:?}", config);

    info!("Starting MCP GDB Server on port {}", config.server_port);

    tools::init_gdb_manager();

    let server_protocol = Server::builder("MCP Server GDB".to_string(), "0.1.0".to_string())
        .capabilities(ServerCapabilities {
            tools: Some(json!({
                "listChanged": false,
            })),
            ..Default::default()
        });

    let server_protocol = register_tools(server_protocol).build();

    match args.transport {
        TransportType::Stdio => {
            let transport = Arc::new(
                Box::new(ServerStdioTransport::new(server_protocol)) as Box<dyn Transport>
            );
            let mut transport_guard = TRANSPORT.lock().await;
            *transport_guard = Some(transport.clone());
            transport.open().await
        }
        TransportType::Sse => {
            let transport = Arc::new(Box::new(ServerSseTransport::new(
                "127.0.0.1".to_string(),
                config.server_port,
                server_protocol,
            )) as Box<dyn Transport>);
            let mut transport_guard = TRANSPORT.lock().await;
            *transport_guard = Some(transport.clone());
            transport.open().await
        }
    }
}

/// Register all debugging tools to the server
fn register_tools(builder: ServerProtocolBuilder) -> ServerProtocolBuilder {
    builder
        .register_tool(
            tools::CreateSessionTool::tool(),
            tools::CreateSessionTool::call(),
        )
        .register_tool(tools::GetSessionTool::tool(), tools::GetSessionTool::call())
        .register_tool(
            tools::GetAllSessionsTool::tool(),
            tools::GetAllSessionsTool::call(),
        )
        .register_tool(
            tools::CloseSessionTool::tool(),
            tools::CloseSessionTool::call(),
        )
        .register_tool(
            tools::StartDebuggingTool::tool(),
            tools::StartDebuggingTool::call(),
        )
        .register_tool(
            tools::StopDebuggingTool::tool(),
            tools::StopDebuggingTool::call(),
        )
        .register_tool(
            tools::GetBreakpointsTool::tool(),
            tools::GetBreakpointsTool::call(),
        )
        .register_tool(
            tools::SetBreakpointTool::tool(),
            tools::SetBreakpointTool::call(),
        )
        .register_tool(
            tools::DeleteBreakpointTool::tool(),
            tools::DeleteBreakpointTool::call(),
        )
        .register_tool(
            tools::GetStackFramesTool::tool(),
            tools::GetStackFramesTool::call(),
        )
        .register_tool(
            tools::GetLocalVariablesTool::tool(),
            tools::GetLocalVariablesTool::call(),
        )
        .register_tool(
            tools::ContinueExecutionTool::tool(),
            tools::ContinueExecutionTool::call(),
        )
        .register_tool(
            tools::StepExecutionTool::tool(),
            tools::StepExecutionTool::call(),
        )
        .register_tool(
            tools::NextExecutionTool::tool(),
            tools::NextExecutionTool::call(),
        )
}
