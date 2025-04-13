use std::path::Path;

use ratatui::Frame;
use ratatui::layout::Constraint::{Fill, Length, Min};
use ratatui::layout::Layout;
use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::models::{MemoryType, ResolveSymbol};
use crate::{App, Mode};

pub mod asm;
pub mod bt;
pub mod hexdump;
pub mod mapping;
pub mod output;
pub mod registers;
pub mod stack;
pub mod title;

// Ayu bell colors
const BLUE: Color = Color::Rgb(0x59, 0xc2, 0xff);
const PURPLE: Color = Color::Rgb(0xd2, 0xa6, 0xff);
const ORANGE: Color = Color::Rgb(0xff, 0x8f, 0x40);
const YELLOW: Color = Color::Rgb(0xe6, 0xb4, 0x50);
const GREEN: Color = Color::Rgb(0xaa, 0xd9, 0x4c);
const RED: Color = Color::Rgb(0xff, 0x33, 0x33);
const DARK_GRAY: Color = Color::Rgb(0x20, 0x27, 0x34);
const GRAY: Color = Color::Rgb(0x44, 0x44, 0x44);
const GRAY_FG: Color = Color::Rgb(100, 100, 100);

const HEAP_COLOR: Color = GREEN;
const STACK_COLOR: Color = PURPLE;
const TEXT_COLOR: Color = RED;
const STRING_COLOR: Color = YELLOW;
const ASM_COLOR: Color = ORANGE;

const SAVED_OUTPUT: usize = 10;

/// Amount of stack addresses we save/display
pub const SAVED_STACK: u16 = 14;

pub const SCROLL_CONTROL_TEXT: &str = "(up(k), down(j), 50 up(K), 50 down(J), top(g), bottom(G))";

pub fn ui<'a>(f: &mut Frame<'a>, app: &mut App) {
    // TODO: register size should depend on arch
    let top_size = Fill(1);

    // If only output, then no top and fill all with output
    if let Mode::OnlyOutput = app.mode {
        let output_size = Fill(1);
        let vertical = Layout::vertical([Length(2), output_size]);
        let [title_area, output] = vertical.areas(f.area());

        title::draw_title_area(app, f, title_area);
        output::draw_output(app, f, output, true);
        return;
    }

    // the rest will include the top
    let output_size = Length(SAVED_OUTPUT as u16);

    let bt_len = app.bt.len();
    let top = if bt_len == 0 {
        let vertical = Layout::vertical([Length(2), top_size, output_size]);
        let [title_area, top, output] = vertical.areas(f.area());

        title::draw_title_area(app, f, title_area);
        output::draw_output(app, f, output, false);

        top
    } else {
        let vertical = Layout::vertical([
            Length(2),
            top_size,
            Length(bt_len as u16 + 1),
            output_size,
            Length(3),
        ]);
        let [title_area, top, bt_area, output] = vertical.areas(f.area());

        bt::draw_bt(app, f, bt_area);
        title::draw_title_area(app, f, title_area);
        output::draw_output(app, f, output, false);

        top
    };

    match app.mode {
        Mode::All => {
            let register_size = Min(10);
            let stack_size = Length(10 + 1);
            // 5 previous, 5 now + after
            let asm_size = Length(11);
            let vertical = Layout::vertical([register_size, stack_size, asm_size]);
            let [register, stack, asm] = vertical.areas(top);

            registers::draw_registers(app, f, register);
            stack::draw_stack(app, f, stack);
            asm::draw_asm(app, f, asm);
        }
        Mode::OnlyRegister => {
            let vertical = Layout::vertical([Fill(1)]);
            let [all] = vertical.areas(top);
            registers::draw_registers(app, f, all);
        }
        Mode::OnlyStack => {
            let vertical = Layout::vertical([Fill(1)]);
            let [all] = vertical.areas(top);
            stack::draw_stack(app, f, all);
        }
        Mode::OnlyInstructions => {
            let vertical = Layout::vertical([Fill(1)]);
            let [all] = vertical.areas(top);
            asm::draw_asm(app, f, all);
        }
        Mode::OnlyMapping => {
            let vertical = Layout::vertical([Fill(1)]);
            let [all] = vertical.areas(top);
            mapping::draw_mapping(app, f, all);
        }
        Mode::OnlyHexdump => {
            let vertical = Layout::vertical([Fill(1)]);
            let [all] = vertical.areas(top);
            hexdump::draw_hexdump(app, f, all);
        }
        _ => (),
    }
}

/// Apply color to val
pub fn apply_val_color(span: &mut Span, memory_type: MemoryType) {
    match memory_type {
        MemoryType::Stack => {
            span.style = Style::new().fg(STACK_COLOR);
        }
        MemoryType::Heap => {
            span.style = Style::new().fg(HEAP_COLOR);
        }
        MemoryType::Exec => {
            span.style = Style::new().fg(TEXT_COLOR);
        }
        _ => (),
    }
}

/// Add resolve symbol value to span
pub fn add_resolve_symbol_to_span<'a>(
    resolve_symbol: &ResolveSymbol,
    spans: &mut Vec<Span<'a>>,
    app: &App,
    filepath: &Path,
    longest_cells: &mut usize,
    width: usize,
) {
    for (i, v) in resolve_symbol.map.iter().enumerate() {
        // check if ascii
        if *v > 0xff {
            let bytes = (*v).to_le_bytes();
            if bytes
                .iter()
                .all(|a| a.is_ascii_alphabetic() || a.is_ascii_graphic() || a.is_ascii_whitespace())
            {
                // if we detect it's ascii, the rest is ascii
                let mut full_s = String::new();
                for r in resolve_symbol.map.iter().skip(i) {
                    let bytes = (*r).to_le_bytes();
                    if let Ok(s) = std::str::from_utf8(&bytes) {
                        full_s.push_str(s);
                    }
                }
                let cell =
                    Span::from(format!("→ \"{}\"", full_s)).style(Style::new().fg(STRING_COLOR));
                spans.push(cell);
                return;
            }
        }

        // if not, it's a value
        let hex_string = format!("0x{:02x}", v);
        let hex_width = hex_string.len();
        let padding_width = width.saturating_sub(hex_width);
        let mut span =
            Span::from(format!("→ {}{:padding$}", hex_string, "", padding = padding_width));
        let memory_type = app.classify_val(*v, filepath);
        apply_val_color(&mut span, memory_type);
        spans.push(span);
    }
    if resolve_symbol.repeated_pattern {
        spans.push(Span::from("→ [loop detected]").style(Style::new().fg(GRAY)));
    }
    if !resolve_symbol.final_assembly.is_empty() {
        spans.push(
            Span::from(format!("→ {:width$}", resolve_symbol.final_assembly, width = width))
                .style(Style::new().fg(ASM_COLOR)),
        );
    }
    if spans.len() > *longest_cells {
        *longest_cells = spans.len();
    }
}
