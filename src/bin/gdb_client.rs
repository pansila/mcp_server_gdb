use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use mcp_core::{
    client::ClientBuilder,
    transport::{ClientSseTransportBuilder, ClientStdioTransport},
    types::{ClientCapabilities, Implementation, ToolResponseContent},
};
use serde_json::json;
use tracing::info;
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

    // Create client
    let stdio_client = if args.transport == TransportType::Stdio {
        let transport =
            ClientStdioTransport::new("./target/debug/mcp_server_gdb", &["--log-level", "debug"])?;
        Some(ClientBuilder::new(transport).build())
    } else {
        None
    };

    let sse_client = if args.transport == TransportType::Sse {
        let url = format!("http://{}:{}", args.server_host, args.server_port);
        let transport = ClientSseTransportBuilder::new(url).build();
        Some(ClientBuilder::new(transport).build())
    } else {
        None
    };

    // Clone executable in advance to avoid move issues
    let executable_clone1 = args.executable.clone();
    let executable_clone2 = args.executable.clone();

    // Choose client based on transport type
    match args.transport {
        TransportType::Stdio => {
            let client = stdio_client.unwrap();

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

            // Create GDB session
            let session_response = client
                .call_tool(
                    "create_session",
                    executable_clone1.map(|path| json!({ "executable_path": path })),
                )
                .await?;

            info!("Session creation response: {:?}", session_response);

            // Extract session ID from response
            let content = session_response.content.first().unwrap();
            let session_id;
            if let ToolResponseContent::Text { text } = content {
                session_id = text.split_once(": ").unwrap().1.split('"').next().unwrap();
            } else {
                bail!("Unable to parse session ID");
            }

            info!("Session ID: {}", session_id);

            // Set breakpoint
            if let Some(executable) = &executable_clone2 {
                info!("Setting breakpoint at test_app.rs:5");
                let breakpoint_response = client
                    .call_tool(
                        "set_breakpoint",
                        Some(json!({
                            "session_id": session_id,
                            "file": "test_app.rs",
                            "line": 5
                        })),
                    )
                    .await?;
                info!("Breakpoint response: {:?}", breakpoint_response);
            }

            // Start debugging
            info!("Starting debugging");
            let start_response = client
                .call_tool(
                    "start_debugging",
                    Some(json!({
                        "session_id": session_id,
                        "timeout": 10
                    })),
                )
                .await?;
            info!("Start debugging response: {:?}", start_response);

            // Get stack frames
            info!("Getting stack frames");
            let frames_response = client
                .call_tool(
                    "get_stack_frames",
                    Some(json!({
                        "session_id": session_id
                    })),
                )
                .await?;
            info!("Stack frames response: {:?}", frames_response);

            // Close session
            info!("Closing session");
            let close_response = client
                .call_tool(
                    "close_session",
                    Some(json!({
                        "session_id": session_id
                    })),
                )
                .await?;
            info!("Close session response: {:?}", close_response);
        }
        TransportType::Sse => {
            let client = sse_client.unwrap();

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

            // Create GDB session
            let session_response = client
                .call_tool(
                    "create_session",
                    executable_clone1.map(|path| json!({ "executable_path": path })),
                )
                .await?;

            info!("Session creation response: {:?}", session_response);

            // Extract session ID from response
            let response_text = format!("{:?}", session_response);
            let session_id = response_text
                .split(": ")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .ok_or_else(|| anyhow::anyhow!("Unable to parse session ID"))?
                .to_string();

            info!("Session ID: {}", session_id);

            // Set breakpoint
            if let Some(executable) = &executable_clone2 {
                info!("Setting breakpoint at test_app.rs:5");
                let breakpoint_response = client
                    .call_tool(
                        "set_breakpoint",
                        Some(json!({
                            "session_id": session_id,
                            "file": "test_app.rs",
                            "line": 5
                        })),
                    )
                    .await?;
                info!("Breakpoint response: {:?}", breakpoint_response);
            }

            // Start debugging
            info!("Starting debugging");
            let start_response = client
                .call_tool(
                    "start_debugging",
                    Some(json!({
                        "session_id": session_id,
                        "timeout": 10
                    })),
                )
                .await?;
            info!("Start debugging response: {:?}", start_response);

            // Get stack frames
            info!("Getting stack frames");
            let frames_response = client
                .call_tool(
                    "get_stack_frames",
                    Some(json!({
                        "session_id": session_id
                    })),
                )
                .await?;
            info!("Stack frames response: {:?}", frames_response);

            // Close session
            info!("Closing session");
            let close_response = client
                .call_tool(
                    "close_session",
                    Some(json!({
                        "session_id": session_id
                    })),
                )
                .await?;
            info!("Close session response: {:?}", close_response);
        }
    }

    Ok(())
}
