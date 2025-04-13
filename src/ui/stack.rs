use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::{ORANGE, PURPLE, add_resolve_symbol_to_span};
use crate::App;

pub fn draw_stack<'a>(app: &App, f: &mut Frame<'a>, stack: Rect) {
    let block = Block::default().borders(Borders::TOP).title("Stack".fg(ORANGE));
    let mut lines = vec![];
    let mut longest_cells = 0;
    let width: usize = if app.bit32 { 11 } else { 19 };

    let empty = PathBuf::from("");
    let filepath = app.filepath.as_ref().unwrap_or(&empty);
    for (addr, values) in app.stack.iter() {
        let hex_string = format!("0x{:02x}", addr);
        let hex_width = hex_string.len();
        let padding_width = (width - 4).saturating_sub(hex_width);
        let span = Span::from(format!("  {}{:padding$}", hex_string, "", padding = padding_width))
            .style(Style::new().fg(PURPLE));
        let mut spans = vec![span];
        add_resolve_symbol_to_span(values, &mut spans, app, filepath, &mut longest_cells, width);
        let line = Line::from(spans);
        lines.push(line);
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, stack);
}
