use std::any::Any;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use mcp_core::{
    client::{Client, ClientBuilder},
    transport::{ClientSseTransport, ClientSseTransportBuilder, ClientStdioTransport},
    types::{ClientCapabilities, Implementation, ToolResponseContent},
};
use serde_json::{Value, json};
use tracing::{debug, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
enum TransportType {
    Stdio,
    Sse,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Transport type
    #[arg(value_enum, default_value_t = TransportType::Stdio)]
    transport: TransportType,

    /// Server address (only for SSE transport)
    #[arg(long, default_value = "127.0.0.1")]
    server_host: String,

    /// Server port (only for SSE transport)
    #[arg(long, default_value = "8080")]
    server_port: u16,

    /// Executable file path
    #[arg(short, long)]
    executable: Option<String>,
}

// Helper function to call the call_tool method on any type of client
async fn call_tool(
    client: &Box<dyn Any>,
    tool_name: &str,
    params: Option<Value>,
) -> Result<Vec<ToolResponseContent>> {
    info!("Calling tool: {}", tool_name);
    debug!("Params: {:?}", params);
    if let Some(client) = client.downcast_ref::<Client<ClientStdioTransport>>() {
        let response = client.call_tool(tool_name, params).await?;
        Ok(response.content)
    } else if let Some(client) = client.downcast_ref::<Client<ClientSseTransport>>() {
        let response = client.call_tool(tool_name, params).await?;
        Ok(response.content)
    } else {
        bail!("Unknown client type")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args = Args::parse();

    // Initialize logging
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::try_new(&args.log_level).unwrap_or_else(|_| EnvFilter::new("info"))
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting GDB client");

    // Create client based on transport type
    let client: Box<dyn Any> = match args.transport {
        TransportType::Stdio => {
            let transport = ClientStdioTransport::new(
                "./target/debug/mcp_server_gdb",
                &["--log-level", "debug"],
            )?;
            let client = ClientBuilder::new(transport).build();

            // Connect to server
            client.open().await?;

            // Initialize client
            client
                .initialize(
                    Implementation {
                        name: "gdb-client".to_string(),
                        version: "1.0".to_string(),
                    },
                    ClientCapabilities::default(),
                )
                .await?;

            Box::new(client)
        }
        TransportType::Sse => {
            let url = format!("http://{}:{}", args.server_host, args.server_port);
            let transport = ClientSseTransportBuilder::new(url).build();
            let client = ClientBuilder::new(transport).build();

            // Connect to server
            client.open().await?;

            Box::new(client)
        }
    };

    // Create GDB session
    let session_response = call_tool(
        &client,
        "create_session",
        args.executable
            .map(|path| json!({ "executable_path": path })),
    )
    .await?;

    info!("Session creation response: {:?}", session_response);

    // Extract session ID from response
    let content = session_response.first().unwrap();
    let session_id;
    if let ToolResponseContent::Text { text } = content {
        session_id = text.split_once(": ").unwrap().1.split('"').next().unwrap();
    } else {
        bail!("Unable to parse session ID");
    }

    info!("Session ID: {}", session_id);

    // Set breakpoint
    let breakpoint_response = call_tool(
        &client,
        "set_breakpoint",
        Some(json!({
            "session_id": session_id,
            "file": "test_app.rs",
            "line": 5
        })),
    )
    .await?;
    info!("Breakpoint response: {:?}", breakpoint_response);

    // Start debugging
    let start_response = call_tool(
        &client,
        "start_debugging",
        Some(json!({
            "session_id": session_id,
            "timeout": 10
        })),
    )
    .await?;
    info!("Start debugging response: {:?}", start_response);

    // Get stack frames
    let frames_response = call_tool(
        &client,
        "get_stack_frames",
        Some(json!({
            "session_id": session_id
        })),
    )
    .await?;
    info!("Stack frames response: {:?}", frames_response);

    // Close session
    let close_response = call_tool(
        &client,
        "close_session",
        Some(json!({
            "session_id": session_id
        })),
    )
    .await?;
    info!("Close session response: {:?}", close_response);

    Ok(())
}
