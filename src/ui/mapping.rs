use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Row, Scrollbar, ScrollbarOrientation, Table};

use super::{BLUE, ORANGE, SCROLL_CONTROL_TEXT};
use crate::App;

pub fn draw_mapping<'a>(app: &mut App, f: &mut Frame<'a>, mapping_rect: Rect) {
    let title = format!("Memory Mapping {SCROLL_CONTROL_TEXT}");

    let mut rows = vec![];
    rows.push(
        Row::new(["Start Address", "End Address", "Size", "Offset", "Permissions", "Path"])
            .style(Style::new().fg(BLUE)),
    );
    if let Some(memory_map) = app.memory_map.as_ref() {
        for m in memory_map {
            let row = Row::new([
                format!("0x{:08x}", m.start_address),
                format!("0x{:08x}", m.end_address),
                format!("0x{:08x}", m.size),
                format!("0x{:08x}", m.offset),
                m.permissions.clone().unwrap_or("".to_string()),
                m.path.as_ref().map_or("".to_string(), |p| p.to_string_lossy().to_string()),
            ]);
            rows.push(row);
        }
    }
    let len = rows.len();
    let max = mapping_rect.height;
    let skip = if len <= max as usize { 0 } else { app.memory_map_scroll.scroll };

    let memory_map_scroll = &mut app.memory_map_scroll;
    memory_map_scroll.scroll = len;
    memory_map_scroll.state.last();
    let rows: Vec<Row> = rows.into_iter().skip(skip).take(max as usize).collect();

    let widths = [
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Fill(1),
    ];
    let block = Block::default().borders(Borders::ALL).title(title.fg(ORANGE));
    let table = Table::new(rows, widths).block(block);
    f.render_widget(table, mapping_rect);
    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        mapping_rect,
        &mut memory_map_scroll.state,
    );
}
