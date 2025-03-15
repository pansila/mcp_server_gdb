/// Server Configuration
pub struct Config {
    /// Server port
    pub server_port: u16,
    /// GDB path
    pub gdb_path: String,
    /// Temporary file directory
    pub temp_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("Invalid server port"),
            gdb_path: std::env::var("GDB_PATH").unwrap_or_else(|_| "gdb".to_string()),
            temp_dir: std::env::temp_dir().to_string_lossy().to_string(),
        }
    }
}
