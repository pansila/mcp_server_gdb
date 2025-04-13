use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{ORANGE, PURPLE, RED, add_resolve_symbol_to_span, apply_val_color};
use crate::App;
use crate::models::{RegisterRaw, TrackedRegister};

/// Registers
pub fn draw_registers<'a>(app: &App, f: &mut Frame<'a>, register: Rect) {
    let block = Block::default().borders(Borders::TOP).title("Registers".fg(ORANGE));

    let mut lines = vec![];
    let mut longest_register_name = 0;
    let mut longest_extra_val = 0;

    let regs = app.registers.clone();
    if regs.is_empty() {
        f.render_widget(block, register);
        return;
    }

    // find longest register name
    // TODO: cache this
    let reg_changed = app.register_changed.clone();
    for TrackedRegister { register, resolve: _ } in regs.iter() {
        if let Some(reg) = register {
            if let (Some(name), Some(_)) = (&reg.name, &reg.value) {
                longest_register_name = name.len();
            }
        }
    }
    let width: usize = if app.bit32 { 11 } else { 19 };

    let empty = PathBuf::from("");
    let filepath = app.filepath.as_ref().unwrap_or(&empty);
    for (i, TrackedRegister { register, resolve }) in regs.iter().enumerate() {
        if let Some(reg) = register {
            if let (Some(name), Some(value)) = (&reg.name, &reg.value) {
                if let RegisterRaw::U64(val) = value {
                    let changed = reg_changed.contains(&(i as u8));
                    let mut reg_name =
                        Span::from(format!("  {name:width$}", width = longest_register_name))
                            .style(Style::new().fg(PURPLE));
                    let memory_type = app.classify_val(val.0, filepath);

                    let mut extra_derefs = Vec::new();
                    add_resolve_symbol_to_span(
                        &resolve,
                        &mut extra_derefs,
                        app,
                        filepath,
                        &mut longest_extra_val,
                        width,
                    );

                    let hex_string = format!("{}", val.0);
                    let hex_width = hex_string.len();
                    let padding_width = width.saturating_sub(hex_width);
                    let mut span = Span::from(format!(
                        "→ {}{:padding$}",
                        hex_string,
                        "",
                        padding = padding_width
                    ));
                    apply_val_color(&mut span, memory_type);

                    // Apply color to reg name
                    if changed {
                        reg_name = reg_name.style(Style::new().fg(RED));
                    }
                    let mut line = Line::from(vec![reg_name, span]);
                    line.spans.append(&mut extra_derefs);
                    lines.push(line);
                }
            }
        }
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, register);
}
