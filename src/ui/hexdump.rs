use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation};

use super::{BLUE, DARK_GRAY, GREEN, ORANGE, SCROLL_CONTROL_TEXT, YELLOW};
use crate::models::{RegisterRaw, TrackedRegister};
use crate::{App, Endian};

pub const HEXDUMP_WIDTH: usize = 16;

/// Convert bytes in hexdump, `skip` that many lines, `take` that many lines
fn to_hexdump_str<'a>(
    app: &mut App,
    pos: u64,
    buffer: &[u8],
    skip: usize,
    take: usize,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    for (offset, chunk) in buffer.chunks(16).skip(skip).take(take).enumerate() {
        let mut hex_spans = Vec::new();
        // bytes
        for byte in chunk.iter() {
            let color = color(*byte);
            hex_spans.push(Span::styled(format!("{:02x} ", byte), Style::default().fg(color)));
        }

        // ascii
        hex_spans.push(Span::raw("| "));
        for byte in chunk.iter() {
            let ascii_char = if byte.is_ascii_graphic() { *byte as char } else { '.' };
            let color = color(*byte);
            hex_spans.push(Span::styled(ascii_char.to_string(), Style::default().fg(color)));
        }

        // check if value has a register reference
        let thirty = app.bit32;

        let mut ref_spans = Vec::new();
        let registers = app.registers.clone();

        ref_spans.push(Span::raw("| "));

        // NOTE: This is disabled, since it's mostly useless?
        //deref_bytes_to_registers(&endian, chunk, thirty, &mut ref_spans, &registers);

        let windows = if thirty { 4 } else { 8 };
        for r in registers.iter() {
            if let Some(reg) = &r.register {
                if let (Some(name), Some(reg_value)) = (&reg.name, &reg.value) {
                    if let RegisterRaw::U64(val) = reg_value {
                        for n in 0..=windows {
                            if val.0 as usize
                                == pos as usize + ((offset + skip) * HEXDUMP_WIDTH + n)
                            {
                                ref_spans.push(Span::raw(format!(
                                    "← ${}(0x{:02x}) ",
                                    name.clone(),
                                    val.0
                                )));
                            }
                        }
                    }
                }
            }
        }

        let line = Line::from_iter(
            vec![Span::raw(format!("{:08x}: ", (skip + offset) * HEXDUMP_WIDTH)), Span::raw("")]
                .into_iter()
                .chain(hex_spans)
                .chain(ref_spans),
        );

        lines.push(line);
    }

    lines
}

fn deref_bytes_to_registers(
    endian: &Option<Endian>,
    chunk: &[u8],
    thirty: bool,
    ref_spans: &mut Vec<Span<'_>>,
    registers: &Vec<TrackedRegister>,
) {
    let windows = if thirty { 4 } else { 8 };
    for w in chunk.windows(windows) {
        let bytes_val = if thirty {
            let val = if endian.unwrap() == Endian::Big {
                // TODO: try_into()
                u32::from_be_bytes([w[0], w[1], w[2], w[3]])
            } else {
                u32::from_le_bytes([w[0], w[1], w[2], w[3]])
            };

            val as u64
        } else {
            if endian.unwrap() == Endian::Big {
                u64::from_be_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]])
            } else {
                u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]])
            }
        };

        for r in registers.iter() {
            if let Some(reg) = &r.register {
                if let (Some(name), Some(reg_value)) = (&reg.name, &reg.value) {
                    if let RegisterRaw::U64(val) = reg_value {
                        if val.0 != 0 {
                            // Find registers that are pointing to the value at a byte offset
                            if bytes_val == val.0 {
                                ref_spans.push(Span::raw(format!(
                                    "${}(0x{:02x?}) ",
                                    name.clone(),
                                    val.0
                                )));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn color(byte: u8) -> Color {
    if byte == 0x00 {
        DARK_GRAY
    } else if byte.is_ascii_graphic() {
        BLUE
    } else if byte.is_ascii_whitespace() {
        GREEN
    } else if byte.is_ascii() {
        ORANGE
    } else {
        YELLOW
    }
}

fn popup_area(area: Rect, percent_x: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(3)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

fn block(pos: &str) -> Block {
    let block = Block::default().borders(Borders::ALL).title(
        format!("Hexdump{pos} {SCROLL_CONTROL_TEXT}, Save(S), HEAP(H), STACK(T))").fg(ORANGE),
    );
    block
}

pub fn draw_hexdump<'a>(app: &mut App, f: &mut Frame<'a>, hexdump: Rect) {
    let hexdump_active = app.hexdump.is_some();
    let mut pos = "".to_string();

    if hexdump_active {
        let r = app.hexdump.clone().unwrap();
        pos = format!("(0x{:02x?})", r.0);
        let data = &r.1;

        let skip = app.hexdump_scroll.scroll;
        let take = hexdump.height;
        let lines = to_hexdump_str(app, r.0, data, skip as usize, take as usize);
        let content_len = data.len() / HEXDUMP_WIDTH;

        let lines: Vec<Line> = lines.into_iter().collect();
        let hexdump_scroll = &mut app.hexdump_scroll;
        hexdump_scroll.scroll = content_len;
        hexdump_scroll.state.last();
        let paragraph =
            Paragraph::new(lines).block(block(&pos)).style(Style::default().fg(Color::White));

        f.render_widget(paragraph, hexdump);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            hexdump,
            &mut hexdump_scroll.state,
        );
    } else {
        f.render_widget(Paragraph::new("").block(block(&pos)), hexdump);
    }
}
