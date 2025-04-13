use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Scrollbar, ScrollbarOrientation};

use super::{BLUE, SCROLL_CONTROL_TEXT};
use crate::App;

pub fn draw_output<'a>(app: &mut App, f: &mut Frame<'a>, output: Rect, full: bool) {
    let len = app.output.len();
    let max = output.height;
    let skip = if full {
        if len <= max as usize { 0 } else { app.output_scroll.scroll }
    } else if len <= max as usize {
        0
    } else {
        len - max as usize + 2
    };

    let output_scroll = &mut app.output_scroll;
    output_scroll.scroll = len;
    output_scroll.state.last();

    let outputs: Vec<ListItem> = app
        .output
        .iter()
        .skip(skip)
        .take(max as usize)
        .map(|m| {
            let m = m.replace('\t', "    ");
            let content = vec![Line::from(Span::raw(m.to_string()))];
            ListItem::new(content)
        })
        .collect();
    let help = if full { SCROLL_CONTROL_TEXT } else { "" };
    let output_block = List::new(outputs)
        .block(Block::default().borders(Borders::ALL).title(format!("Output {help}").fg(BLUE)));
    f.render_widget(output_block, output);

    // only show scrollbar on full page
    if full {
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            output,
            &mut output_scroll.state,
        );
    }
}
