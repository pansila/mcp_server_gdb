#[derive(Debug)]
/// Server Configuration
pub struct Config {
    /// Server port
    pub server_port: u16,
    /// GDB command execution timeout in seconds
    pub command_timeout: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("Invalid server port"),
            command_timeout: std::env::var("GDB_COMMAND_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        }
    }
}
