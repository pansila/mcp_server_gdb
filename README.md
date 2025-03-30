# MCP Server GDB

A GDB/MI protocol server based on the MCP protocol, providing remote application debugging capabilities with AI assistants.

## Features

- Create and manage GDB debug sessions
- Set and manage breakpoints
- View stack information and variables
- Control program execution (run, pause, step, etc.)
- Support concurrent multi-session debugging

## Installation

### Pre-built Binaries
Find the binaries in the release page, choose one per your working platform, then you can run it directly.

### Build From Source
Clone the repository and build it by cargo
```
cargo build --release
cargo run
```

## Usage

1. Just run it directly: `./mcp-server-gdb`
2. The server supports two transport modes:
   - Stdio (default): Standard input/output transport
   - SSE: Server-Sent Events transport, default at `http://127.0.0.1:8080`

## Configuration

You can adjust server configuration by modifying the `src/config.rs` file or by environment variables:

- Server IP Address
- Server port
- GDB command timeout time (in seconds)

## Supported MCP Tools

### Session Management

- `create_session` - Create a new GDB debugging session
- `get_session` - Get specific session information
- `get_all_sessions` - Get all sessions
- `close_session` - Close session

### Debug Control

- `start_debugging` - Start debugging
- `stop_debugging` - Stop debugging
- `continue_execution` - Continue execution
- `step_execution` - Step into next line
- `next_execution` - Step over next line

### Breakpoint Management

- `get_breakpoints` - Get breakpoint list
- `set_breakpoint` - Set breakpoint
- `delete_breakpoint` - Delete breakpoint

### Debug Information

- `get_stack_frames` - Get stack frame information
- `get_local_variables` - Get local variables
- `get_registers` - Get registers

## License

MIT
