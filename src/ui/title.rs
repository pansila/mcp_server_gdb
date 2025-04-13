use ratatui::Frame;
use ratatui::layout::Constraint::Length;
use ratatui::layout::{Alignment, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};

use super::{ASM_COLOR, GRAY_FG, GREEN, HEAP_COLOR, STACK_COLOR, STRING_COLOR, TEXT_COLOR};
use crate::App;

pub fn draw_title_area(app: &App, f: &mut Frame, title_area: Rect) {
    let vertical_title = Layout::vertical([Length(1), Length(1)]);
    let [first, second] = vertical_title.areas(title_area);
    f.render_widget(
        Block::new()
            .borders(Borders::TOP)
            .title_alignment(Alignment::Center)
            .title(vec![
                "|".fg(GRAY_FG),
                env!("CARGO_PKG_NAME").bold(),
                "-".fg(GRAY_FG),
                "v".into(),
                env!("CARGO_PKG_VERSION").into(),
                "|".fg(GRAY_FG),
            ])
            .title(
                Line::from(vec![
                    Span::raw(" | "),
                    Span::styled(
                        "Heap",
                        Style::default().fg(HEAP_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::styled(
                        "Stack",
                        Style::default().fg(STACK_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::styled(
                        "Code",
                        Style::default().fg(TEXT_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::styled(
                        "String",
                        Style::default().fg(STRING_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                    Span::styled(
                        "Asm",
                        Style::default().fg(ASM_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | "),
                ])
                .right_aligned(),
            ),
        first,
    );

    let tab = Tabs::new(vec![
        "F1 Main",
        "F2 Registers",
        "F3 Stack",
        "F4 Instructions",
        "F5 Output",
        "F6 Mapping",
        "F7 Hexdump",
    ])
    .block(Block::new().title_alignment(Alignment::Center))
    .style(Style::default())
    .highlight_style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD))
    .select(app.mode as usize)
    .divider("|");

    f.render_widget(tab, second);
}
