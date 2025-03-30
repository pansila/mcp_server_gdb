use core::fmt;
use std::{
    collections::HashMap,
    fmt::Display,
    ops::{Add, Sub},
    path::PathBuf,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::AppError,
    mi::{commands::BreakPointNumber, output::ResultRecord},
};

/// GDB session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDBSession {
    /// Session ID
    pub id: String,
    /// Session status
    pub status: GDBSessionStatus,
    /// Creation time
    pub created_at: u64,
}

/// GDB session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GDBSessionStatus {
    /// Created but not started
    Created,
    /// Running
    Running,
    /// Program stopped at breakpoint
    Stopped,
    /// Session terminated
    Terminated,
}

/// GDB command request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GDBCommandRequest {
    /// GDB/MI command
    pub command: String,
}

/// Create session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// Executable file path (optional)
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SrcPosition {
    pub file: PathBuf,
    pub line: usize,
}

impl SrcPosition {
    pub const fn new(file: PathBuf, line: usize) -> Self {
        SrcPosition { file, line }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Address(pub usize);
impl FromStr for Address {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        usize::from_str_radix(&s[2..], 16).map(Address)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

impl Add<usize> for Address {
    type Output = Self;
    fn add(self, rhs: usize) -> Self {
        Address(self.0 + rhs)
    }
}

impl Sub<usize> for Address {
    type Output = Self;
    fn sub(self, rhs: usize) -> Self {
        Address(self.0 - rhs)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakPoint {
    pub number: BreakPointNumber,
    pub address: Option<Address>,
    pub enabled: bool,
    pub src_pos: Option<SrcPosition>, // not present if debug information is missing!
    pub r#type: String,
    pub display: String,
}

impl TryFrom<&Value> for BreakPoint {
    type Error = AppError;

    fn try_from(bkpt: &Value) -> Result<Self, Self::Error> {
        let number = bkpt["number"]
            .as_str()
            .ok_or(AppError::ParseError("find bp number".to_string()))?
            .parse::<BreakPointNumber>()
            .map_err(|e| AppError::ParseError(e.to_string()))?;
        let enabled =
            bkpt["enabled"].as_str().ok_or(AppError::ParseError("find enabled".to_string()))?
                == "y";
        let address = bkpt["addr"].as_str().and_then(|addr| Address::from_str(addr).ok()); //addr may not be present or contain
        let src_pos = {
            let maybe_file = bkpt["fullname"].as_str();
            let maybe_line = bkpt["line"]
                .as_str()
                .map(|l_nr| l_nr.parse::<usize>().map_err(|e| AppError::ParseError(e.to_string())));
            if let (Some(file), Some(line)) = (maybe_file, maybe_line) {
                Some(SrcPosition::new(PathBuf::from(file), line?))
            } else {
                None
            }
        };
        let r#type =
            bkpt["type"].as_str().ok_or(AppError::ParseError("find type".to_string()))?.to_string();
        let display = bkpt["disp"]
            .as_str()
            .ok_or(AppError::ParseError("find display".to_string()))?
            .to_string();
        Ok(BreakPoint { number, address, enabled, src_pos, r#type, display })
    }
}

pub struct BreakPointSet {
    map: HashMap<BreakPointNumber, BreakPoint>,
    pub last_change: std::time::Instant,
}

impl Default for BreakPointSet {
    fn default() -> Self {
        Self { map: HashMap::new(), last_change: std::time::Instant::now() }
    }
}

impl BreakPointSet {
    fn notify_change(&mut self) {
        self.last_change = std::time::Instant::now();
    }

    pub fn update_breakpoint(&mut self, new_bp: BreakPoint) {
        let _ = self.map.insert(new_bp.number, new_bp);
        //debug_assert!(res.is_some(), "Modified non-existent breakpoint");
        self.notify_change();
    }

    pub fn remove_breakpoint(&mut self, bp_num: BreakPointNumber) {
        self.map.remove(&bp_num);
        if bp_num.minor.is_none() {
            //TODO: ensure removal of child breakpoints
        }
        self.notify_change();
    }
}

impl std::ops::Deref for BreakPointSet {
    type Target = HashMap<BreakPointNumber, BreakPoint>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

/// Stack frame information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame level
    pub level: u32,
    /// Function name
    pub function: String,
    /// File name
    pub file: Option<String>,
    /// Line number
    pub line: Option<u32>,
    /// Address
    pub address: String,
}

/// Variable information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable value
    pub value: String,
    /// Variable type
    pub type_name: Option<String>,
}
