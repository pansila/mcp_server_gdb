# MCP Server GDB

A GDB/MI protocol server based on the MCP protocol, providing remote application debugging capabilities.

## Features

- Create and manage GDB debug sessions
- Set and manage breakpoints
- View stack information and variables
- Control program execution (run, pause, step, etc.)
- Support concurrent multi-session debugging

## Supported MCP Tools

### Session Management

- `create_session` - Create a new GDB debugging session
  - Parameters: `executable_path` (optional) - Path to the executable to debug
- `get_session` - Get specific session information
  - Parameters: `session_id` - GDB session ID
- `get_all_sessions` - Get all sessions
  - Parameters: none
- `close_session` - Close session
  - Parameters: `session_id` - GDB session ID

### Debug Control

- `start_debugging` - Start debugging
  - Parameters: `session_id` - GDB session ID
- `stop_debugging` - Stop debugging
  - Parameters: `session_id` - GDB session ID
- `continue_execution` - Continue execution
  - Parameters: `session_id` - GDB session ID
- `step_execution` - Step into next line
  - Parameters: `session_id` - GDB session ID
- `next_execution` - Step over next line
  - Parameters: `session_id` - GDB session ID

### Breakpoint Management

- `get_breakpoints` - Get breakpoint list
  - Parameters: `session_id` - GDB session ID
- `set_breakpoint` - Set breakpoint
  - Parameters: 
    - `session_id` - GDB session ID
    - `file` - Source file path
    - `line` - Line number
- `delete_breakpoint` - Delete breakpoint
  - Parameters: 
    - `session_id` - GDB session ID
    - `breakpoint_id` - Breakpoint ID

### Debug Information

- `get_stack_frames` - Get stack frame information
  - Parameters: `session_id` - GDB session ID
- `get_local_variables` - Get local variables
  - Parameters: 
    - `session_id` - GDB session ID
    - `frame_id` - Stack frame ID

## Usage

1. Install Rust and Cargo
2. Clone this repository
3. Run `cargo run` to start the server
4. The server supports two transport modes:
   - Stdio (default): Standard input/output transport
   - SSE: Server-Sent Events transport, default at `http://127.0.0.1:8080`

## Configuration

You can adjust server configuration by modifying the `src/config.rs` file or by environment variables:

- Server port
- GDB path
- Temporary file directory

## License

MIT
