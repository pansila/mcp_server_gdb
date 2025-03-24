use std::ffi::OsString;
use std::fmt;
use std::io::Error;
use std::path::Path;

use tokio::io::AsyncWriteExt;
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct MiCommand {
    pub operation: &'static str,
    pub options: Option<Vec<OsString>>,
    pub parameters: Option<Vec<OsString>>,
}

pub enum DisassembleMode {
    DisassemblyOnly = 0,
    DisassemblyWithRawOpcodes = 2,
    MixedSourceAndDisassembly = 1, // deprecated and 4 would be preferred, but might not be available in older gdb(mi) versions
    MixedSourceAndDisassemblyWithRawOpcodes = 3, // deprecated and 5 would be preferred, same as above
}

pub enum WatchMode {
    Read,
    Write,
    Access,
}

pub enum BreakPointLocation<'a> {
    Address(usize),
    Function(&'a Path, &'a str),
    Line(&'a Path, usize),
}

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub struct BreakPointNumber {
    pub major: usize,
    pub minor: Option<usize>,
}

impl std::str::FromStr for BreakPointNumber {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(dot_pos) = s.find('.') {
            let major = s[..dot_pos].parse::<usize>().map_err(|e| e.to_string())?;
            let minor = s[dot_pos + 1..]
                .parse::<usize>()
                .map_err(|e| e.to_string())?;
            Ok(BreakPointNumber {
                major,
                minor: Some(minor),
            })
        } else {
            match s.parse::<usize>() {
                Ok(val) => Ok(BreakPointNumber {
                    major: val,
                    minor: None,
                }),
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

impl fmt::Display for BreakPointNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(minor) = self.minor {
            write!(f, "{}.{}", self.major, minor)
        } else {
            write!(f, "{}", self.major)
        }
    }
}

fn escape_command(input: &str) -> String {
    let mut output = '\"'.to_string();
    for c in input.chars() {
        match c {
            '\\' => output.push_str("\\\\"),
            '\"' => output.push_str("\\\""),
            '\r' => output.push_str("\\\r"),
            '\n' => output.push_str("\\\n"),
            other => output.push(other),
        }
    }
    output.push('\"');
    output
}

impl MiCommand {
    pub async fn write_interpreter_string<S: AsyncWriteExt + Unpin>(
        &self,
        sink: &mut S,
        token: u64,
    ) -> Result<(), Error> {
        // use std::os::unix::ffi::OsStrExt;
        let mut command = OsString::new();
        if !self.operation.is_empty() {
            command.push(format!("{}-{}", token, self.operation));
        }

        if let Some(options) = &self.options {
            for option in options {
                command.push(" ");
                command.push(option);
            }
        }
        if let Some(parameters) = &self.parameters {
            if self.options.is_some() {
                command.push(" --");
            }
            for parameter in parameters {
                command.push(" ");
                command.push(parameter);
            }
        }
        command.push("\n");
        info!(
            "Writing GDB command: {}",
            String::from_utf8_lossy(command.as_encoded_bytes())
        );

        sink.write_all(command.as_encoded_bytes()).await?;
        Ok(())
    }

    pub fn interpreter_exec<S1: Into<OsString>, S2: Into<OsString>>(
        interpreter: S1,
        command: S2,
    ) -> MiCommand {
        MiCommand {
            operation: "interpreter-exec",
            options: Some(vec![interpreter.into(), command.into()]),
            parameters: None,
        }
    }

    pub fn cli_exec(command: &str) -> MiCommand {
        Self::interpreter_exec("console".to_owned(), escape_command(command))
    }

    pub fn data_disassemble_file<P: AsRef<Path>>(
        file: P,
        linenum: usize,
        lines: Option<usize>,
        mode: DisassembleMode,
    ) -> MiCommand {
        MiCommand {
            operation: "data-disassemble",
            options: Some(vec![
                OsString::from("-f"),
                OsString::from(file.as_ref()),
                OsString::from("-l"),
                OsString::from(linenum.to_string()),
                OsString::from("-n"),
                OsString::from(lines.map(|l| l as isize).unwrap_or(-1).to_string()),
            ]),
            parameters: Some(vec![OsString::from((mode as u8).to_string())]),
        }
    }

    pub fn data_disassemble_address(
        start_addr: usize,
        end_addr: usize,
        mode: DisassembleMode,
    ) -> MiCommand {
        MiCommand {
            operation: "data-disassemble",
            options: Some(vec![
                OsString::from("-s"),
                OsString::from(start_addr.to_string()),
                OsString::from("-e"),
                OsString::from(end_addr.to_string()),
            ]),
            parameters: Some(vec![OsString::from((mode as u8).to_string())]),
        }
    }

    pub fn data_evaluate_expression(expression: String) -> MiCommand {
        MiCommand {
            operation: "data-evaluate-expression",
            options: Some(vec![OsString::from(format!("\"{}\"", expression))]), //TODO: maybe we need to quote existing " in expression. Is this even possible?
            parameters: None,
        }
    }

    pub fn insert_breakpoint(location: BreakPointLocation) -> MiCommand {
        MiCommand {
            operation: "break-insert",
            options: match location {
                BreakPointLocation::Address(addr) => {
                    Some(vec![OsString::from(format!("*0x{:x}", addr))])
                }
                BreakPointLocation::Function(path, func_name) => {
                    let mut ret = OsString::from(path);
                    ret.push(":");
                    ret.push(func_name);
                    Some(vec![ret])

                    // Not available in old gdb(mi) versions
                    //vec![
                    //    OsString::from("--source"),
                    //    OsString::from(path),
                    //    OsString::from("--function"),
                    //    OsString::from(func_name),
                    //]
                }
                BreakPointLocation::Line(path, line_number) => {
                    let mut ret = OsString::from(path);
                    ret.push(":");
                    ret.push(line_number.to_string());
                    Some(vec![ret])

                    // Not available in old gdb(mi) versions
                    //vec![
                    //OsString::from("--source"),
                    //OsString::from(path),
                    //OsString::from("--line"),
                    //OsString::from(format!("{}", line_number)),
                    //],
                }
            },
            parameters: None,
        }
    }

    pub fn delete_breakpoints(breakpoint_numbers: Vec<OsString>) -> MiCommand {
        //let options = options: breakpoint_numbers.map(|n| format!("{} ", n)).collect(),
        //GDB is broken: see http://sourceware-org.1504.n7.nabble.com/Bug-breakpoints-20133-New-unable-to-delete-a-sub-breakpoint-td396197.html
        let mut options = breakpoint_numbers;
        options.sort();
        options.dedup();
        MiCommand {
            operation: "break-delete",
            options: Some(options),
            parameters: None,
        }
    }

    pub fn breakpoints_list() -> MiCommand {
        MiCommand {
            operation: "break-list",
            ..Default::default()
        }
    }

    pub fn insert_watchpoing(expression: &str, mode: WatchMode) -> MiCommand {
        let options = match mode {
            WatchMode::Write => None,
            WatchMode::Read => Some(vec!["-r".into()]),
            WatchMode::Access => Some(vec!["-a".into()]),
        };
        MiCommand {
            operation: "break-watch",
            options,
            parameters: Some(vec![expression.into()]),
        }
    }

    pub fn environment_pwd() -> MiCommand {
        MiCommand {
            operation: "environment-pwd",
            ..Default::default()
        }
    }

    // Be aware: This does not seem to always interrupt execution.
    // Use gdb.interrupt_execution instead.
    pub fn exec_interrupt() -> MiCommand {
        MiCommand {
            operation: "exec-interrupt",
            ..Default::default()
        }
    }

    pub fn exec_run() -> MiCommand {
        MiCommand {
            operation: "exec-run",
            ..Default::default()
        }
    }

    pub fn exec_continue() -> MiCommand {
        MiCommand {
            operation: "exec-continue",
            ..Default::default()
        }
    }

    pub fn exec_step() -> MiCommand {
        MiCommand {
            operation: "exec-step",
            ..Default::default()
        }
    }

    pub fn exec_next() -> MiCommand {
        MiCommand {
            operation: "exec-next",
            ..Default::default()
        }
    }

    // Warning: This cannot be used to pass special characters like \n to gdb because
    // (unlike it is said in the spec) there is apparently no way to pass \n unescaped
    // to gdb, and for "exec-arguments" gdb somehow does not unescape these chars...
    pub fn exec_arguments(args: Vec<OsString>) -> MiCommand {
        MiCommand {
            operation: "exec-arguments",
            options: Some(args),
            parameters: None,
        }
    }

    pub fn exit() -> MiCommand {
        MiCommand {
            operation: "gdb-exit",
            ..Default::default()
        }
    }

    pub fn select_frame(frame_number: u64) -> MiCommand {
        MiCommand {
            operation: "stack-select-frame",
            options: Some(vec![frame_number.to_string().into()]),
            parameters: None,
        }
    }

    pub fn stack_info_frame(frame_number: Option<u64>) -> MiCommand {
        MiCommand {
            operation: "stack-info-frame",
            options: if let Some(frame_number) = frame_number {
                Some(vec![frame_number.to_string().into()])
            } else {
                None
            },
            parameters: None,
        }
    }

    pub fn stack_info_depth() -> MiCommand {
        MiCommand {
            operation: "stack-info-depth",
            ..Default::default()
        }
    }

    pub fn stack_list_variables(
        thread_number: Option<usize>,
        frame_number: Option<usize>,
    ) -> MiCommand {
        let mut parameters = vec![];
        if let Some(thread_number) = thread_number {
            parameters.push("--thread".into());
            parameters.push(thread_number.to_string().into());
        }
        if let Some(frame_number) = frame_number {
            parameters.push("--frame".into());
            parameters.push(frame_number.to_string().into());
        }
        parameters.push("--simple-values".into()); //TODO: make configurable if required.
        MiCommand {
            operation: "stack-list-variables",
            options: None,
            parameters: Some(parameters),
        }
    }

    pub fn stack_list_frames(low_frame: Option<usize>, high_frame: Option<usize>) -> MiCommand {
        let options = if let Some(low) = low_frame {
            if let Some(high) = high_frame {
                if low > high {
                    Some(vec![high.to_string().into(), low.to_string().into()])
                } else {
                    Some(vec![low.to_string().into(), high.to_string().into()])
                }
            } else {
                // large enough number to include all frames, only existing frames will be shown
                Some(vec![low.to_string().into(), String::from("99999").into()])
            }
        } else {
            if let Some(high) = high_frame {
                Some(vec![String::from("0").into(), high.to_string().into()])
            } else {
                None
            }
        };
        MiCommand {
            operation: "stack-list-frames",
            options,
            parameters: None,
        }
    }

    pub fn thread_info(thread_id: Option<u64>) -> MiCommand {
        MiCommand {
            operation: "thread-info",
            options: if let Some(id) = thread_id {
                Some(vec![id.to_string().into()])
            } else {
                None
            },
            parameters: None,
        }
    }

    pub fn file_exec_and_symbols(file: &Path) -> MiCommand {
        MiCommand {
            operation: "file-exec-and-symbols",
            options: Some(vec![file.into()]),
            parameters: None,
        }
    }

    pub fn file_symbol_file(file: Option<&Path>) -> MiCommand {
        MiCommand {
            operation: "file-symbol-file",
            options: if let Some(file) = file {
                Some(vec![file.into()])
            } else {
                None
            },
            parameters: None,
        }
    }

    pub fn list_thread_groups(list_all_available: bool, thread_group_ids: &[u32]) -> MiCommand {
        MiCommand {
            operation: "list-thread-groups",
            options: if list_all_available {
                Some(vec![OsString::from("--available")])
            } else {
                None
            },
            parameters: Some(
                thread_group_ids
                    .iter()
                    .map(|id| id.to_string().into())
                    .collect(),
            ),
        }
    }

    pub fn var_create(
        name: Option<OsString>, /*none: generate name*/
        expression: &str,
        frame_addr: Option<u64>, /*none: current frame*/
    ) -> MiCommand {
        MiCommand {
            operation: "var-create",
            options: None,
            parameters: Some(vec![
                name.unwrap_or_else(|| "\"-\"".into()),
                frame_addr
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "\"*\"".to_string())
                    .into(),
                escape_command(expression).into(),
            ]),
        }
    }
    pub fn var_delete(name: impl Into<OsString>, delete_children: bool) -> MiCommand {
        let mut parameters = vec![];
        if delete_children {
            parameters.push("-c".into());
        }
        parameters.push(name.into());
        MiCommand {
            operation: "var-delete",
            options: None,
            parameters: Some(parameters),
        }
    }
    pub fn var_list_children(
        name: impl Into<OsString>,
        print_values: bool,
        from_to: Option<std::ops::Range<u64>>,
    ) -> MiCommand {
        let mut cmd = MiCommand {
            operation: "var-list-children",
            options: None,
            parameters: Some(vec![
                if print_values {
                    "--all-values"
                } else {
                    "--no-values"
                }
                .into(),
                name.into(),
            ]),
        };
        if let (Some(from_to), Some(params)) = (from_to, &mut cmd.parameters) {
            params.push(OsString::from(from_to.start.to_string()));
            params.push(OsString::from(from_to.end.to_string()));
        }
        cmd
    }

    pub fn empty() -> MiCommand {
        MiCommand {
            operation: "",
            ..Default::default()
        }
    }
}
