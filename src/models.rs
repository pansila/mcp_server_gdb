use core::fmt;
use std::collections::{HashMap, VecDeque};
use std::fmt::Display;
use std::ops::{Add, Sub};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::char;
use nom::character::complete::{alphanumeric1, space0};
use nom::combinator::map;
use nom::sequence::{delimited, preceded, separated_pair};
use nom::{IResult, Parser};
use serde::{Deserialize, Serialize, de};
use serde_with::{DisplayFromStr, serde_as, skip_serializing_none};
use tracing::debug;

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
pub struct Address<T: FromStr<Err = std::num::ParseIntError> + Display + Clone + std::fmt::LowerHex>(
    pub T,
);

impl<T: FromStr<Err = std::num::ParseIntError> + Display + Clone + std::fmt::LowerHex> From<String>
    for Address<T>
{
    fn from(s: String) -> Self {
        let s = if s.starts_with("0x") { &s[2..] } else { &s };
        match u128::from_str_radix(s, 16) {
            Ok(val) => {
                Address(T::from_str(&val.to_string()).unwrap_or_else(|_| T::from_str("0").unwrap()))
            }
            Err(_) => Address(T::from_str(s).unwrap_or_else(|_| T::from_str("0").unwrap())),
        }
    }
}

impl<T: FromStr<Err = std::num::ParseIntError> + Display + Clone + std::fmt::LowerHex>
    From<Address<T>> for String
{
    fn from(addr: Address<T>) -> Self {
        format!("0x{:x}", addr.0)
    }
}

impl<
    T: FromStr<Err = std::num::ParseIntError> + Display + Clone + std::fmt::LowerHex + Add<Output = T>,
> Add<T> for Address<T>
{
    type Output = Self;
    fn add(self, rhs: T) -> Self {
        Address(self.0 + rhs)
    }
}

impl<
    T: FromStr<Err = std::num::ParseIntError> + Display + Clone + std::fmt::LowerHex + Sub<Output = T>,
> Sub<T> for Address<T>
{
    type Output = Self;
    fn sub(self, rhs: T) -> Self {
        Address(self.0 - rhs)
    }
}

// Type aliases for common address sizes
pub type Address32 = Address<u32>;
pub type Address64 = Address<u64>;
pub type Address128 = Address<u128>;

impl Address128 {
    pub fn new(low: Address64, high: Address64) -> Self {
        let mut val = Address::<u128>(high.0 as u128);
        val.0 = val.0 << 64;
        val.0 += low.0 as u128;
        val
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

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakPoint {
    pub number: BreakPointNumber,
    #[serde(rename = "addr")]
    pub address: Option<Address64>,
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
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame level
    #[serde_as(as = "DisplayFromStr")]
    pub level: u32,
    /// Function name
    #[serde(rename = "func")]
    pub function: String,
    /// File name
    pub file: Option<String>,
    /// Full name of the file
    pub fullname: Option<String>,
    /// Line number
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub line: Option<u32>,
    /// Address
    #[serde(rename = "addr")]
    pub address: Option<Address64>,
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
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable type, only present if --all-values or --simple-values
    pub r#type: Option<String>,
    /// Variable value, only present if --simple-values
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum RegisterRaw {
    U32(Address32),
    U64(Address64),
    U128(Address128),
    U256(Address128, Address128),
}

// Define Register struct to hold register data
#[serde_as]
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Register {
    // Not exist in the register value output but can be amended afterwards
    pub name: Option<String>,
    #[serde_as(as = "DisplayFromStr")]
    pub number: usize,
    pub value: Option<RegisterRaw>,
    pub v2_int128: Option<String>,
    pub v8_int32: Option<String>,
    pub v4_int64: Option<String>,
    pub v8_float: Option<String>,
    pub v16_int8: Option<String>,
    pub v4_int32: Option<String>,
    pub error: Option<String>,
}

// impl Register {
//     /// Value is not set to anything readable
//     pub fn is_set(&self) -> bool {
//         self.error.is_none() && self.value !=
// Some("<unavailable>".to_string())     }
// }

fn pair<'a>()
-> impl Parser<&'a str, Output = (&'a str, &'a str), Error = nom::error::Error<&'a str>> {
    delimited(
        char('['),
        separated_pair(alphanumeric1, (char(','), space0), alphanumeric1),
        char(']'),
    )
}

fn register_data(input: &str) -> IResult<&str, RegisterRaw> {
    let v128bits = separated_pair(tag("v2_int64"), (char(':'), space0), pair());
    let v256bits = separated_pair(tag("v2_int128"), (char(':'), space0), pair());
    map(
        preceded(take_until("v2_int"), alt((v128bits, v256bits))),
        |(r#type, (v1, v2))| match r#type {
            "v2_int64" => RegisterRaw::U128(Address128::new(
                Address64::from(v1.to_owned()),
                Address64::from(v2.to_owned()),
            )),
            "v2_int128" => {
                RegisterRaw::U256(Address128::from(v1.to_owned()), Address128::from(v2.to_owned()))
            }
            _ => unreachable!(),
        },
    )
    .parse(input)
}

impl<'de> Deserialize<'de> for RegisterRaw {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: String = serde::Deserialize::deserialize(deserializer)?;
        if s.starts_with("0x") {
            Ok(RegisterRaw::U64(Address64::from(s[2..].to_owned())))
        } else {
            register_data(&s).map(|(_, o)| o).map_err(|e| de::Error::custom(e.to_string()))
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BT {
    pub location: u64,
    pub function: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASM {
    pub address: u64,
    pub inst: String,
    pub offset: u64,
    pub func_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrackedRegister {
    pub register: Option<Register>,
    pub resolve: ResolveSymbol,
}

impl TrackedRegister {
    pub fn new(register: Option<Register>, resolve: ResolveSymbol) -> Self {
        Self { register, resolve }
    }
}

pub enum MemoryType {
    Unknown,
    Stack,
    Heap,
    Exec,
}

// Define memory output layout
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// The start address of the memory block, as hexadecimal literal.
    pub begin: String,
    /// The end address of the memory block, as hexadecimal literal.
    pub end: String,
    /// The offset of the memory block, as hexadecimal literal, relative to the
    /// start address passed to -data-read-memory-bytes.
    pub offset: String,
    /// The contents of the memory block, in hex bytes.
    pub contents: String,
}

#[derive(Debug, Clone)]
pub struct MemoryMapping {
    pub start_address: u64,
    pub end_address: u64,
    pub size: u64,
    pub offset: u64,
    pub permissions: Option<String>,
    pub path: Option<PathBuf>,
}

impl MemoryMapping {
    /// Mapping is the stack
    pub fn is_stack(&self) -> bool {
        self.path.as_ref().map_or(false, |p| p == Path::new("[stack]"))
    }

    /// Mapping is the heap
    pub fn is_heap(&self) -> bool {
        self.path.as_ref().map_or(false, |p| p == Path::new("[heap]"))
    }

    /// Mapping filepath matches `filepath`
    ///
    /// This could be set from something like "file
    /// test-assets/test_render_app/a.out" so we make sure to match with a
    /// mapping such as: "/home/wcampbell/projects/wcampbell/heretek/
    /// test-assets/test_render_app/a.out"
    pub fn is_path(&self, filepath: &Path) -> bool {
        if let Some(path) = &self.path { path.ends_with(filepath) } else { false }
    }

    pub fn is_exec(&self) -> bool {
        if let Some(permissions) = &self.permissions { permissions.contains('x') } else { false }
    }

    /// Mapping contains the `addr`
    pub fn contains(&self, addr: u64) -> bool {
        (self.start_address..self.end_address).contains(&addr)
    }
}

impl MemoryMapping {
    /// Parse from `MEMORY_MAP_START_STR_NEW`
    fn from_str_new(line: &str) -> Result<Self, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some([start_address, end_address, size, offset, permissions]) = parts.first_chunk() {
            Ok(MemoryMapping {
                start_address: u64::from_str_radix(&start_address[2..], 16)
                    .map_err(|_| "Invalid start address")?,
                end_address: u64::from_str_radix(&end_address[2..], 16)
                    .map_err(|_| "Invalid end address")?,
                size: u64::from_str_radix(&size[2..], 16).map_err(|_| "Invalid size")?,
                offset: u64::from_str_radix(&offset[2..], 16).map_err(|_| "Invalid offset")?,
                permissions: Some(permissions.to_string()),
                path: None,
            })
        } else if let Some([start_address, end_address, size, offset, permissions, path]) =
            parts.first_chunk()
        {
            Ok(MemoryMapping {
                start_address: u64::from_str_radix(&start_address[2..], 16)
                    .map_err(|_| "Invalid start address")?,
                end_address: u64::from_str_radix(&end_address[2..], 16)
                    .map_err(|_| "Invalid end address")?,
                size: u64::from_str_radix(&size[2..], 16).map_err(|_| "Invalid size")?,
                offset: u64::from_str_radix(&offset[2..], 16).map_err(|_| "Invalid offset")?,
                permissions: Some(permissions.to_string()),
                path: Some(PathBuf::from(path)),
            })
        } else {
            return Err(format!("Invalid line format: {}", line));
        }
    }

    /// Parse from `MEMORY_MAP_START_STR_OLD`
    fn from_str_old(line: &str) -> Result<Self, String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some([start_address, end_address, size, offset, path]) = parts.first_chunk() {
            Ok(MemoryMapping {
                start_address: u64::from_str_radix(&start_address[2..], 16)
                    .map_err(|_| "Invalid start address")?,
                end_address: u64::from_str_radix(&end_address[2..], 16)
                    .map_err(|_| "Invalid end address")?,
                size: u64::from_str_radix(&size[2..], 16).map_err(|_| "Invalid size")?,
                offset: u64::from_str_radix(&offset[2..], 16).map_err(|_| "Invalid offset")?,
                permissions: None,
                path: Some(PathBuf::from(path)),
            })
        } else {
            Err(format!("Invalid line format: {}", line))
        }
    }
}

/// Parse from `MEMORY_MAP_START_STR_NEW`
pub fn parse_memory_mappings_new(input: &str) -> Vec<MemoryMapping> {
    input.lines().skip(1).filter_map(|line| MemoryMapping::from_str_new(line).ok()).collect()
}

/// Parse from `MEMORY_MAP_START_STR_OLD`
pub fn parse_memory_mappings_old(input: &str) -> Vec<MemoryMapping> {
    input.lines().skip(1).filter_map(|line| MemoryMapping::from_str_old(line).ok()).collect()
}

#[derive(Debug, Clone)]
pub struct ResolveSymbol {
    pub map: VecDeque<u64>,
    pub repeated_pattern: bool,
    pub final_assembly: String,
}

impl ResolveSymbol {
    /// Attempts to insert a `u64` value and prevents repeated patterns
    ///
    /// Returns `true` if inserted, `false` otherwise.
    pub fn try_push(&mut self, value: u64) -> bool {
        self.map.push_back(value);

        if self.has_repeating_pattern() {
            self.repeated_pattern = true;
            self.map.pop_back();
            return false;
        }

        true
    }

    fn has_repeating_pattern(&self) -> bool {
        if self.map.len() == 1 {
            return false;
        }
        if self.map.len() == 2 {
            return self.map[0] == self.map[1];
        }

        debug!("map: {:02x?}", self.map);
        for pattern_length in 2..=self.map.len() / 2 {
            for start in 0..(self.map.len() - pattern_length) {
                let first_section: &Vec<u64> =
                    &self.map.range(start..start + pattern_length).cloned().collect();
                debug!("1: {:02x?}", first_section);

                for second_start in start + 1..(self.map.len() - pattern_length + 1) {
                    let second_section: &Vec<u64> = &self
                        .map
                        .range(second_start..second_start + pattern_length)
                        .cloned()
                        .collect();
                    debug!("2: {:02x?}", second_section);
                    if first_section == second_section {
                        debug!("found matching");
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_address() {
        #[derive(Deserialize)]
        struct Test {
            addr: Address<u64>,
            opt_addr: Option<Address<u64>>,
        }
        let test: Test =
            serde_json::from_str("{\"addr\": \"0x1234abcd\", \"opt_addr\":\"0xABCD1234\"}")
                .unwrap();
        assert_eq!(test.addr, Address(0x1234abcd));
        assert_eq!(test.opt_addr, Some(Address(0xabcd1234)));
    }

    #[test]
    fn test_register_normal_value() {
        #[derive(Deserialize)]
        struct Test {
            reg: Register,
        }
        let test: Test =
            serde_json::from_str("{\"reg\":{\"number\": \"1\", \"value\": \"0x1234\"}}").unwrap();
        assert_eq!(test.reg.number, 1);
    }

    #[test]
    fn test_register_composite_value() {
        #[derive(Deserialize)]
        struct Test {
            reg: Register,
        }
        let test: Test =
            serde_json::from_str("{\"reg\":{\"number\": \"1\", \"value\": \"builtin_type_vec256i {v16_bfloat16: [0x4, 0x0, 0x0, 0x0, 0x7c80, 0x556b, 0x5555, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0], v16_half: [0x4, 0x0, 0x0, 0x0, 0x7c80, 0x556b, 0x5555, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0], v8_float: [0x4, 0x0, 0x556b7c80, 0x5555, 0x0, 0x0, 0x0, 0x0], v4_double: [0x4, 0x5555556b7c80, 0x0, 0x0], v32_int8: [0x4, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x80, 0x7c, 0x6b, 0x55, 0x55, 0x55, 0x0 <repeats 18 times>], v16_int16: [0x4, 0x0, 0x0, 0x0, 0x7c80, 0x556b, 0x5555, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0], v8_int32: [0x4, 0x0, 0x556b7c80, 0x5555, 0x0, 0x0, 0x0, 0x0], v4_int64: [0x4, 0x5555556b7c80, 0x0, 0x0], v2_int128: [0x5555556b7c800000000000000004, 0x0]}\"}}").unwrap();
        assert_eq!(
            test.reg.value,
            Some(RegisterRaw::U256(
                Address::<u128>(0x5555556b7c800000000000000004),
                Address::<u128>(0)
            ))
        );
    }
}
