use super::Token;

// use std::io::{BufRead, BufReader, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use anyhow::Result;
use json::JsonValue;
use json::object::Object;
use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_while_m_n};
use nom::character::complete::{char, digit1, line_ending, multispace1};
use nom::combinator::{map, map_opt, map_res, opt, value, verify};
use nom::error::{FromExternalError, ParseError};
use nom::multi::{fold, many0, separated_list0};
use nom::sequence::{delimited, preceded};
use nom::{IResult, Parser};
use tracing::{debug, error, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultClass {
    Done,
    Running,
    Connected,
    Error,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakPointEvent {
    Created,
    Deleted,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadEvent {
    Created,
    GroupStarted,
    Exited,
    GroupExited,
    Selected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncClass {
    Stopped,
    CmdParamChanged,
    LibraryLoaded,
    Thread(ThreadEvent),
    BreakPoint(BreakPointEvent),
    Other(String), //?
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncKind {
    Exec,
    Status,
    Notify,
}

#[derive(Debug, Clone)]
pub enum StreamKind {
    Console,
    Target,
    Log,
}

#[derive(Debug, Clone)]
pub struct ResultRecord {
    pub(crate) token: Option<Token>,
    pub class: ResultClass,
    pub results: Object,
}

#[derive(Debug, Clone)]
pub enum OutOfBandRecord {
    AsyncRecord {
        token: Option<Token>,
        kind: AsyncKind,
        class: AsyncClass,
        results: Object,
    },
    StreamRecord {
        kind: StreamKind,
        data: String,
    },
}

#[derive(Debug, Clone)]
enum Output {
    Result(ResultRecord),
    OutOfBand(OutOfBandRecord),
    GDBLine,
    SomethingElse(String), /* Debug */
}

// use crate::mi::OutOfBandRecordSink;

use tokio::sync::mpsc::Sender;

pub async fn process_output<T: AsyncRead + Unpin>(
    output: T,
    result_pipe: Sender<ResultRecord>,
    out_of_band_pipe: Sender<OutOfBandRecord>,
    is_running: Arc<AtomicBool>,
) {
    let mut reader = BufReader::new(output);

    loop {
        let mut buffer = String::new();
        match reader.read_line(&mut buffer).await {
            Ok(0) => {
                return;
            }
            Ok(_) => {
                info!("{}", buffer.trim_end());

                let parse_result = match Output::parse(&buffer) {
                    Ok(r) => r,
                    Err(e) => {
                        error!("PARSING ERROR: {}", e);
                        continue;
                    }
                };
                debug!("{:?}", &parse_result);
                match parse_result {
                    Output::Result(record) => {
                        match record.class {
                            ResultClass::Running => is_running.store(true, Ordering::SeqCst),
                            //Apparently sometimes gdb first claims to be running, only to then stop again (without notifying the user)...
                            ResultClass::Error => is_running.store(false, Ordering::SeqCst),
                            _ => {}
                        }
                        result_pipe.send(record).await.expect("send result to pipe");
                    }
                    Output::OutOfBand(record) => {
                        if let OutOfBandRecord::AsyncRecord {
                            class: AsyncClass::Stopped,
                            ..
                        } = record
                        {
                            is_running.store(false, Ordering::SeqCst);
                        }
                        out_of_band_pipe
                            .send(record)
                            .await
                            .expect("send out of band record to pipe");
                    }
                    Output::GDBLine => {}
                    //Output::SomethingElse(_) => { /*println!("SOMETHING ELSE: {}", str);*/ }
                    Output::SomethingElse(text) => {
                        out_of_band_pipe
                            .send(OutOfBandRecord::StreamRecord {
                                kind: StreamKind::Target,
                                data: text,
                            })
                            .await
                            .expect("send out of band record to pipe");
                    }
                }
            }
            Err(e) => {
                panic!("{}", e);
            }
        }
    }
}

impl Output {
    fn parse(line: &str) -> Result<Self, String> {
        match output(line) {
            Ok((_, c)) => Ok(c),
            Err(e) => match e {
                nom::Err::Incomplete(e) => Err(format!("parsing line: incomplete {:?}", e)),
                nom::Err::Error(e) => Err(format!("parse error: {}", e)),
                nom::Err::Failure(e) => Err(format!("parse failure: {}", e)),
            },
        }
    }
}

/// parse the result class by looking for the corresponding tag, which is
/// one of: done, running, connected, error, exit
fn result_class(input: &str) -> IResult<&str, ResultClass> {
    alt((
        value(ResultClass::Done, tag("done")),
        value(ResultClass::Running, tag("running")),
        value(ResultClass::Connected, tag("connected")),
        value(ResultClass::Error, tag("error")),
        value(ResultClass::Exit, tag("exit")),
    ))
    .parse(input)
}

/// Parse a unicode sequence, of the form u{XXXX}, where XXXX is 1 to 6
/// hexadecimal numerals. We will combine this later with parse_escaped_char
/// to parse sequences like \u{00AC}.
fn unicode<'a, E>(input: &'a str) -> IResult<&'a str, char, E>
where
    E: ParseError<&'a str> + FromExternalError<&'a str, std::num::ParseIntError>,
{
    let parse_hex = take_while_m_n(1, 6, |c: char| c.is_ascii_hexdigit());

    let parse_delimited_hex = preceded(char('u'), delimited(char('{'), parse_hex, char('}')));

    let parse_u32 = map_res(parse_delimited_hex, move |hex| u32::from_str_radix(hex, 16));

    map_opt(parse_u32, std::char::from_u32).parse(input)
}

/// Parse an escaped character: \n, \t, \r, \u{00AC}, etc.
fn escaped_char(input: &str) -> IResult<&str, char> {
    preceded(
        char('\\'),
        alt((
            unicode,
            value('\n', char('n')),
            value('\r', char('r')),
            value('\t', char('t')),
            value('\u{08}', char('b')),
            value('\u{0C}', char('f')),
            value('\\', char('\\')),
            value('/', char('/')),
            value('"', char('"')),
        )),
    )
    .parse(input)
}

/// Parse a backslash, followed by any amount of whitespace. This is used later
/// to discard any escaped whitespace.
fn escaped_whitespace(input: &str) -> IResult<&str, &str> {
    preceded(char('\\'), multispace1).parse(input)
}

/// Parse a non-empty block of text that doesn't include \ or "
fn literal(input: &str) -> IResult<&str, &str> {
    let not_quote_slash = is_not("\"\\");

    verify(not_quote_slash, |s: &str| !s.is_empty()).parse(input)
}

/// A string fragment contains a fragment of a string being parsed: either
/// a non-empty Literal (a series of non-escaped characters), a single
/// parsed escaped character, or a block of escaped whitespace.
#[derive(Debug, PartialEq, Eq, Clone)]
enum StringFragment<'a> {
    Literal(&'a str),
    EscapedChar(char),
    EscapedWS,
}

/// Combine parse_literal, parse_escaped_whitespace, and parse_escaped_char
/// into a StringFragment.
fn parse_fragment(input: &str) -> IResult<&str, StringFragment> {
    alt((
        map(literal, |s| StringFragment::Literal(s)),
        map(escaped_char, |c| StringFragment::EscapedChar(c)),
        value(StringFragment::EscapedWS, escaped_whitespace),
    ))
    .parse(input)
}

/// Parse a string. Use a loop of parse_fragment and push all of the fragments
/// into an output string.
fn string(input: &str) -> IResult<&str, String> {
    let build_string = fold(0.., parse_fragment, String::new, |mut string, fragment| {
        match fragment {
            StringFragment::Literal(s) => string.push_str(s.as_ref()),
            StringFragment::EscapedChar(c) => string.push(c),
            StringFragment::EscapedWS => {}
        }
        string
    });

    delimited(char('"'), build_string, char('"')).parse(input)
}

fn to_map(v: Vec<(String, JsonValue)>) -> Object {
    //TODO: fix this and parse the map directly
    let mut obj = Object::new();
    for (name, value) in v {
        debug_assert!(obj.get(&name).is_none(), "Duplicate object member!");
        obj.insert(&name, value);
    }
    obj
}

fn to_list(v: Vec<(String, JsonValue)>) -> Vec<JsonValue> {
    //The gdbmi-grammar is really weird...
    //TODO: fix this and parse the map directly
    v.into_iter().map(|(_, value)| value).collect()
}

fn json_value(input: &str) -> IResult<&str, JsonValue> {
    alt((
        map(string, JsonValue::String),
        map(
            delimited(char('{'), separated_list0(char(','), result), char('}')),
            |results| JsonValue::Object(to_map(results)),
        ),
        map(
            delimited(char('['), separated_list0(char(','), json_value), char(']')),
            |values| JsonValue::Array(values),
        ),
        map(
            delimited(char('['), separated_list0(char(','), result), char(']')),
            |values| JsonValue::Array(to_list(values)),
        ),
    ))
    .parse(input)
}

// Don't even ask... Against its spec, gdb(mi) sometimes emits multiple values for a single tuple
// in a comma separated list.
fn buggy_gdb_list_in_result(input: &str) -> IResult<&str, JsonValue> {
    map(
        separated_list0(tag(","), json_value),
        |mut values: Vec<JsonValue>| {
            if values.len() == 1 {
                values
                    .pop()
                    .expect("len == 1 => first element is guaranteed")
            } else {
                JsonValue::Array(values)
            }
        },
    )
    .parse(input)
}

/// key=value, or just a value which is json object
fn result(input: &str) -> IResult<&str, (String, JsonValue)> {
    map(
        (is_not("={}"), tag("="), buggy_gdb_list_in_result),
        |(var, _, val)| (var.to_string(), val),
    )
    .parse(input)
}

fn token(input: &str) -> IResult<&str, Token> {
    map(digit1, |values: &str| values.parse::<u64>().unwrap()).parse(input)
}

/// \[token\] "^" result-class ( "," result )* nl,
/// where result-class is one of: done, running, connected, error, exit,
/// and result is a json object
fn result_record(input: &str) -> IResult<&str, Output> {
    map(
        (
            opt(token),
            char('^'),
            result_class,
            many0((char(','), result)),
        ),
        |(t, _, c, results)| {
            Output::Result(ResultRecord {
                token: t,
                class: c,
                results: to_map(results.into_iter().map(|(_, v)| (v.0, v.1)).collect()),
            })
        },
    )
    .parse(input)
}

fn async_kind(input: &str) -> IResult<&str, AsyncKind> {
    alt((
        value(AsyncKind::Exec, tag("*")),
        value(AsyncKind::Status, tag("+")),
        value(AsyncKind::Notify, tag("=")),
    ))
    .parse(input)
}

fn async_class(input: &str) -> IResult<&str, AsyncClass> {
    alt((
        value(AsyncClass::Stopped, tag("stopped")),
        value(
            AsyncClass::Thread(ThreadEvent::Created),
            tag("thread-created"),
        ),
        value(
            AsyncClass::Thread(ThreadEvent::GroupStarted),
            tag("thread-group-started"),
        ),
        value(
            AsyncClass::Thread(ThreadEvent::Exited),
            tag("thread-exited"),
        ),
        value(
            AsyncClass::Thread(ThreadEvent::GroupExited),
            tag("thread-group-exited"),
        ),
        value(
            AsyncClass::Thread(ThreadEvent::Selected),
            tag("thread-selected"),
        ),
        value(AsyncClass::CmdParamChanged, tag("cmd-param-changed")),
        value(AsyncClass::LibraryLoaded, tag("library-loaded")),
        value(
            AsyncClass::BreakPoint(BreakPointEvent::Created),
            tag("breakpoint-created"),
        ),
        value(
            AsyncClass::BreakPoint(BreakPointEvent::Deleted),
            tag("breakpoint-deleted"),
        ),
        value(
            AsyncClass::BreakPoint(BreakPointEvent::Modified),
            tag("breakpoint-modified"),
        ),
        map(is_not(","), |msg: &str| AsyncClass::Other(msg.to_string())),
    ))
    .parse(input)
}

/// \[token\] async-kind async-class ( "," result )* nl,
/// where async-kind is one of: * (exec), + (status), = (notify),
/// and async-class is one of: stopped, thread-created, thread-group-started, thread-exited, thread-group-exited,
/// thread-selected, cmd-param-changed, library-loaded, breakpoint-created, breakpoint-deleted, breakpoint-modified, other
/// and result is a json object
fn async_record(input: &str) -> IResult<&str, OutOfBandRecord> {
    map(
        (
            opt(token),
            async_kind,
            async_class,
            many0((char(','), result)),
        ),
        |(t, kind, class, results)| OutOfBandRecord::AsyncRecord {
            token: t,
            kind,
            class,
            results: to_map(results.into_iter().map(|(_, v)| (v.0, v.1)).collect()),
        },
    )
    .parse(input)
}

fn stream_kind(input: &str) -> IResult<&str, StreamKind> {
    alt((
        value(StreamKind::Console, tag("~")),
        value(StreamKind::Target, tag("@")),
        value(StreamKind::Log, tag("&")),
    ))
    .parse(input)
}

/// stream-kind string nl,
/// where stream-kind is one of: ~ (console), @ (target), & (log)
fn stream_record(input: &str) -> IResult<&str, OutOfBandRecord> {
    map((stream_kind, string), |(kind, msg)| {
        OutOfBandRecord::StreamRecord { kind, data: msg }
    })
    .parse(input)
}

/// asynchronous records which reported out of band
fn out_of_band_record(input: &str) -> IResult<&str, Output> {
    map(alt((stream_record, async_record)), |record| {
        Output::OutOfBand(record)
    })
    .parse(input)
}

fn gdb_line(input: &str) -> IResult<&str, Output> {
    value(Output::GDBLine, tag("(gdb) ")).parse(input)
}

fn debug_line(input: &str) -> IResult<&str, Output> {
    Ok(("", Output::SomethingElse(input.to_string())))
}

fn output(input: &str) -> IResult<&str, Output> {
    map(
        (
            alt((result_record, out_of_band_record, gdb_line, debug_line)),
            line_ending,
        ),
        |(output, _)| output,
    )
    .parse(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_output() {
        let Ok(output) = Output::parse("=library-loaded,ranges=[{}]\n") else {
            panic!("parse output failed");
        };
        if let Output::OutOfBand(record) = output {
            if let OutOfBandRecord::AsyncRecord {
                kind,
                class,
                results,
                ..
            } = record
            {
                assert_eq!(kind, AsyncKind::Notify);
                assert_eq!(class, AsyncClass::LibraryLoaded);
                assert_eq!(results.len(), 1);
                assert_eq!(
                    results.get("ranges"),
                    Some(&JsonValue::Array(vec![JsonValue::Object(Object::new())]))
                );
            } else {
                panic!("output is not a out of band record");
            }
        }
    }

    #[test]
    fn test_result_record() {
        if let Ok(output) = Output::parse(
            "^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",enabled=\"y\",addr=\"0x0000000000018fdf\",\
                  func=\"test_app::main::{async_block#0}\",file=\"src/bin/test_app.rs\",fullname=\"mcp_server_gdb/src/bin/test_app.rs\",\
                  line=\"5\",thread-groups=[\"i1\"],times=\"0\",original-location=\"test_app.rs:5\"}",
        ) {
            if let Output::Result(result) = output {
                assert_eq!(result.token, None);
                assert_eq!(result.class, ResultClass::Done);
                if let Some(bkpt) = result.results.get("bkpt") {
                    assert_eq!(bkpt["number"], JsonValue::String("1".to_string()));
                    assert_eq!(bkpt["type"], JsonValue::String("breakpoint".to_string()));
                    assert_eq!(bkpt["disp"], JsonValue::String("keep".to_string()));
                    assert_eq!(bkpt["enabled"], JsonValue::String("y".to_string()));
                    assert_eq!(
                        bkpt["addr"],
                        JsonValue::String("0x0000000000018fdf".to_string())
                    );
                }
            }
        }
    }
}
