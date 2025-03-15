# MCP Server GDB

A GDB/MI protocol server based on the Axum framework, providing RESTful API interfaces for remote application debugging.

## Features

- Create and manage GDB debug sessions
- Send GDB/MI commands and get responses
- Set and manage breakpoints
- View stack information and variables
- Control program execution (run, pause, step, etc.)
- Support concurrent multi-session debugging

## API Endpoints

### Session Management

- `GET /api/sessions` - Get all sessions
- `POST /api/sessions` - Create new session
- `GET /api/sessions/:session_id` - Get specific session
- `DELETE /api/sessions/:session_id` - Close session

### Debug Control

- `POST /api/sessions/:session_id/command` - Send GDB command
- `POST /api/sessions/:session_id/start` - Start debugging
- `POST /api/sessions/:session_id/stop` - Stop debugging
- `POST /api/sessions/:session_id/continue` - Continue execution
- `POST /api/sessions/:session_id/step` - Step execution
- `POST /api/sessions/:session_id/next` - Next execution

### Breakpoint Management

- `GET /api/sessions/:session_id/breakpoints` - Get breakpoint list
- `POST /api/sessions/:session_id/breakpoints` - Set breakpoint
- `DELETE /api/sessions/:session_id/breakpoints/:breakpoint_id` - Delete breakpoint

### Debug Information

- `GET /api/sessions/:session_id/stack` - Get stack information
- `GET /api/sessions/:session_id/variables/:frame_id` - Get local variables

## Usage

1. Install Rust and Cargo
2. Clone this repository
3. Run `cargo run` to start the server
4. Default server runs on `http://localhost:8080`

## Configuration

You can adjust server configuration by modifying the `src/config.rs` file:

- Server port
- GDB path
- Temporary file directory

## License

MIT
