use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Clear, Paragraph, Widget},
};

use crate::app::FormFields;
use crate::app::{App, MetadataField, Mode};
use crate::keymap::{self, Action, ModalKey};
use crate::theme;

use super::{
    centered_rect, dim_area, modal_block, modal_body_style, modal_error_style, modal_label_style,
    modal_value_style,
};

const WIDTH: u16 = 60;

pub fn render(app: &App, full_area: Rect, buf: &mut Buffer) {
    dim_area(full_area, buf);

    let Some(Mode::MetadataEdit(modal)) = app.modes.active() else {
        return;
    };

    let visible = MetadataField::visible(&modal.edits);
    let mut lines: Vec<Line> = Vec::with_capacity(visible.len() + 5);
    lines.push(Line::raw(""));

    for field in visible {
        let focused = field == modal.focused;
        let cursor = if focused { "▏" } else { "" };
        let label = format!("{:<16}", field.label());
        lines.push(Line::from(vec![
            Span::styled(label, modal_label_style()),
            Span::styled(
                format!("{}{cursor}", field.value(&modal.edits)),
                modal_value_style(focused),
            ),
        ]));
    }

    lines.push(Line::raw(""));
    if let Some(error) = &modal.error {
        lines.push(
            Line::from(error.as_str())
                .alignment(Alignment::Center)
                .style(modal_error_style()),
        );
        lines.push(Line::raw(""));
    }
    lines.push(
        Line::from(format!(
            "{}/{next} field · {enter} save · {cancel} cancel",
            keymap::modal_display_for(ModalKey::NextField),
            next = keymap::modal_display_for(ModalKey::PrevField),
            enter = keymap::display_for(Action::Activate),
            cancel = keymap::modal_display_for(ModalKey::Cancel),
        ))
        .alignment(Alignment::Center)
        .style(Style::new().fg(theme::text_muted())),
    );

    let height = lines.len() as u16 + 2;
    let popup = centered_rect(WIDTH, height, full_area);
    Widget::render(Clear, popup, buf);

    Paragraph::new(Text::from(lines))
        .style(modal_body_style())
        .block(modal_block(" Edit Metadata ").padding(ratatui::widgets::Padding::horizontal(2)))
        .render(popup, buf);
}
