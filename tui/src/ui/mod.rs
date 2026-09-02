mod footer;
mod header;
mod library;
mod metadata_modal;
mod modals;
mod now_playing;
mod playlists;
mod rows;
mod song_modal;
mod style;
mod youtube_modal;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Paragraph, Widget},
};

use crate::app::{App, Mode, Panel};
use crate::keymap;

pub use rows::{
    allocate_title_artist_widths, PanelHeight, SongListContext, SongListPanelOptions, label_for,
    render_centered_message, render_no_matches, render_song_list_panel, viewport,
};
pub use crate::strings::plural;
pub use style::{
    centered_rect, content_style, dim_area, display_width, focus_style, format_duration,
    format_mtime, marquee_scroll_offset, marker_style, marquee_window, modal_block,
    modal_body_style, modal_error_style, modal_hint_style, modal_label_style, modal_value_style,
    search_title, side_by_side_rect, sort_title, sort_title_widths, styled_list,
    titled_block, titled_block_split, unfocused_style, SELECTED_MARKER,
};

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ]);
        let [header_area, dir_area, body_area, position_area, footer_area] = area.layout(&layout);

        let body_layout =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]);
        let [left_area, now_playing_area] = body_area.layout(&body_layout);

        style::reset_marquee_activity();

        header::render(self, header_area, buf);
        render_dir_input(self, dir_area, buf);
        match self.panel {
            Panel::Library => library::render(self, left_area, buf),
            Panel::Playlists => playlists::render(self, left_area, buf),
        }
        now_playing::render(self, now_playing_area, buf);
        now_playing::render_position(self, position_area, buf);
        footer::render(self, footer_area, buf);

        self.animating.set(style::marquee_active());

        if let Some(mode) = self.modes.active() {
            match mode {
                Mode::ConfirmQuit => modals::render_quit_confirm(area, buf),
                Mode::ConfirmRemove(playlist_id, song_id) => {
                    modals::render_remove_confirm(self, *playlist_id, *song_id, area, buf);
                }
                Mode::Help => modals::render_help_overlay(self, area, buf),
                Mode::SongModal(_) => song_modal::render(self, area, buf),
                Mode::MetadataEdit(_) => metadata_modal::render(self, area, buf),
                Mode::Youtube(_) => youtube_modal::render(self, area, buf),
                Mode::RomanizedArtistConfirm(_) => {
                    modals::render_romanized_artist_confirm(self, area, buf);
                }
                Mode::ChangeDirectory | Mode::SearchLibrary | Mode::SearchPlaylists => {}
            }
        }
    }
}

fn render_dir_input(app: &App, area: Rect, buf: &mut Buffer) {
    let editing = matches!(app.modes.active(), Some(Mode::ChangeDirectory));
    let title = if editing {
        format!(
            " Directory ({} to load, {} to cancel) ",
            keymap::display_for(keymap::Action::Activate),
            keymap::modal_display_for(keymap::ModalKey::Cancel),
        )
    } else {
        format!(
            " Directory {} ",
            crate::keymap::display_for(crate::keymap::Action::ChangeDirectory)
        )
    };
    let border_style = if editing {
        focus_style()
    } else {
        unfocused_style()
    };

    Paragraph::new(app.dir.dir_input.as_str())
        .style(content_style())
        .block(titled_block(title, border_style))
        .render(area, buf);
}
