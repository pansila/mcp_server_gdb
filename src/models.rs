use core::fmt;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::{Add, Sub};
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

use crate::error::AppError;
use crate::mi::commands::BreakPointNumber;

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

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrcPosition {
    pub fullname: PathBuf,
    #[serde_as(as = "DisplayFromStr")]
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Address(pub usize);

impl From<String> for Address {
    fn from(s: String) -> Self {
        let s = if s.starts_with("0x") { &s[2..] } else { &s };
        Address(usize::from_str_radix(s, 16).unwrap_or(0))
    }
}

impl From<Address> for String {
    fn from(addr: Address) -> Self {
        format!("0x{:x}", addr.0)
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
pub struct Enabled(bool);

impl<'de> Deserialize<'de> for Enabled {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        if s == "y" { Ok(Enabled(true)) } else { Ok(Enabled(false)) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakPoint {
    pub number: BreakPointNumber,
    #[serde(rename = "addr")]
    pub address: Option<Address>,
    pub enabled: Enabled,
    #[serde(flatten)]
    pub src_pos: Option<SrcPosition>, // not present if debug information is missing!
    pub r#type: String,
    #[serde(rename = "disp")]
    pub display: String,
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
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame level
    #[serde_as(as = "DisplayFromStr")]
    pub level: u32,
    /// Function name
    pub func: String,
    /// File name
    pub file: Option<String>,
    /// Full name of the file
    pub fullname: Option<String>,
    /// Line number
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub line: Option<u32>,
    /// Address
    #[serde(rename = "addr")]
    pub address: Option<Address>,
    /// Arch
    pub arch: Option<String>,
}

pub enum PrintValue {
    /// print only the names of the variables, equivalent to "--no-values"
    NoValues,
    /// print also their values, equivalent to "--all-values"
    AllValues,
    /// print the name, type and value for simple data types, and the name and
    /// type for arrays, structures and unions, equivalent to "--simple-values"
    SimpleValues,
}

impl FromStr for PrintValue {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<usize>()? {
            0 => Ok(PrintValue::NoValues),
            1 => Ok(PrintValue::AllValues),
            2 => Ok(PrintValue::SimpleValues),
            _ => Err(AppError::InvalidArgument("only 0,1,2 are valid".to_string())),
        }
    }
}

impl Display for PrintValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// Variable information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable type, only present if --all-values or --simple-values
    pub r#type: Option<String>,
    /// Variable value, only present if --simple-values
    pub value: Option<String>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_address() {
        #[derive(Deserialize)]
        struct Test {
            addr: Address,
            opt_addr: Option<Address>,
        }
        let test: Test =
            serde_json::from_str("{\"addr\": \"0x1234abcd\", \"opt_addr\":\"0xABCD1234\"}")
                .unwrap();
        assert_eq!(test.addr, Address(0x1234abcd));
        assert_eq!(test.opt_addr, Some(Address(0xabcd1234)));
    }
}
