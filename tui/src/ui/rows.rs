use lyre_core::{Library, Song, SongId};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, ListItem, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::{
    app::{PlaylistDisplayMode, Row, is_filtering, song_row_count},
    theme,
};

use super::{
    display_width, focus_style, marquee_window, search_title, sort_title, styled_list,
    titled_block_split, unfocused_style, SELECTED_MARKER,
};

const INDENT_UNIT: &str = "  ";
const NOW_PLAYING_MARKER: &str = "♪ ";

const TITLE_WIDTH_RATIO: usize = 70;
const MIN_MARQUEE_WIDTH: usize = 6;
const TITLE_ARTIST_SEPARATOR: &str = " — ";

fn header_style() -> Style {
    Style::new()
        .fg(theme::section_header())
        .add_modifier(Modifier::BOLD)
}

fn title_style(is_current: bool) -> Style {
    if is_current {
        Style::new()
            .fg(theme::title_current())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::title())
    }
}

fn artist_style() -> Style {
    Style::new().fg(theme::artist())
}

fn detail_style() -> Style {
    Style::new().fg(theme::detail())
}

fn playlist_style() -> Style {
    Style::new().fg(theme::playlist_tag())
}

fn separator_style() -> Style {
    Style::new().fg(theme::separator())
}

fn now_playing_marker_style(is_current: bool) -> Style {
    if is_current {
        Style::new()
            .fg(theme::title_current())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::now_playing_marker_idle())
    }
}

fn missing_style() -> Style {
    Style::new().fg(theme::missing_song())
}

pub struct SongLabel<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub detail: Option<String>,
}

pub fn label_for(
    song: &Song,
    category: Option<crate::app::Category>,
    sort: crate::app::Sort,
) -> SongLabel<'_> {
    use crate::app::Sort;

    let detail = match sort {
        Sort::Duration => Some(super::format_duration(song.metadata().duration)),
        Sort::DateModified => Some(super::format_mtime(song.modified())),
        Sort::Title | Sort::Artist | Sort::Path => None,
    };
    let artist = match category {
        Some(crate::app::Category::Artist) => None,
        _ => Some(song.artist()),
    };

    SongLabel {
        title: song.title(),
        artist,
        detail,
    }
}

fn playlist_suffix(
    names: &[String],
    mode: PlaylistDisplayMode,
    available: usize,
) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    match mode {
        PlaylistDisplayMode::Hidden => None,
        PlaylistDisplayMode::Count => Some(format!("󰲹 {}", names.len())),
        PlaylistDisplayMode::Expanded => {
            if available == 0 {
                return None;
            }
            let joined = names.join(", ");
            Some(marquee_window(&joined, available).into_owned())
        }
    }
}

pub fn song_count(rows: &[Row]) -> usize {
    song_row_count(rows)
}

pub type PlaylistLookup<'a> = (PlaylistDisplayMode, &'a dyn Fn(SongId) -> Vec<String>);
pub type GroupLabelLookup<'a> = &'a dyn Fn(&Song) -> Option<String>;

pub struct TitleArtistWidths {
    pub title_max: usize,
    pub artist_max: Option<usize>,
}

pub fn allocate_title_artist_widths(
    available_for_both: usize,
    title_natural: usize,
    artist_natural: Option<usize>,
) -> TitleArtistWidths {
    let Some(artist_natural) = artist_natural else {
        return TitleArtistWidths {
            title_max: available_for_both,
            artist_max: None,
        };
    };

    let min = MIN_MARQUEE_WIDTH.min(available_for_both);
    let title_cap = ((available_for_both * TITLE_WIDTH_RATIO) / 100).max(min);
    let artist_cap = available_for_both.saturating_sub(title_cap);

    match (title_natural <= title_cap, artist_natural <= artist_cap) {
        (true, true) => TitleArtistWidths {
            title_max: title_natural,
            artist_max: Some(artist_natural),
        },
        (true, false) => TitleArtistWidths {
            title_max: title_natural,
            artist_max: Some(
                available_for_both
                    .saturating_sub(title_natural)
                    .max(min),
            ),
        },
        (false, true) => TitleArtistWidths {
            title_max: available_for_both.saturating_sub(artist_natural).max(min),
            artist_max: Some(artist_natural),
        },
        (false, false) => TitleArtistWidths {
            title_max: title_cap,
            artist_max: Some(artist_cap),
        },
    }
}

pub struct Viewport<'a, T> {
    pub items: &'a [T],
    pub first: usize,
    pub selected: Option<usize>,
}

pub struct PanelHeight(pub usize);

pub fn viewport<'a, T>(
    list_state: &mut ListState,
    all: &'a [T],
    height: usize,
) -> Viewport<'a, T> {
    let len = all.len();
    if len == 0 || height == 0 {
        return Viewport {
            items: &[],
            first: 0,
            selected: None,
        };
    }

    let selected = list_state.selected().filter(|&i| i < len);
    let mut offset = list_state.offset().min(len.saturating_sub(1));

    if let Some(sel) = selected {
        if sel < offset {
            offset = sel;
        } else if sel >= offset + height {
            offset = sel + 1 - height;
        }
    }
    offset = offset.min(len.saturating_sub(height.min(len)));

    *list_state.offset_mut() = offset;

    let end = (offset + height).min(len);
    Viewport {
        items: all.get(offset..end).unwrap_or(&[]),
        first: offset,
        selected: selected.map(|s| s - offset),
    }
}

pub struct SongListContext<'a> {
    pub current: Option<SongId>,
    pub library: &'a Library,
    pub playlist_info: Option<PlaylistLookup<'a>>,
}

pub struct SongListPanelOptions<'a> {
    pub visual_range: Option<(usize, usize)>,
    pub title_prefix: &'a str,
    pub category_label: &'a str,
    pub sort_label: &'a str,
    pub search_mode_open: bool,
    pub query: &'a str,
    pub group_label: Option<GroupLabelLookup<'a>>,
}

fn pinned_group_label(
    rows: &[Row],
    start: usize,
    library: &Library,
    group_label: Option<GroupLabelLookup<'_>>,
) -> Option<String> {
    let group_label = group_label?;
    match rows.get(start)? {
        Row::Song(id, _) => library.get(*id).and_then(group_label),
        Row::Header(_) => None,
    }
}

pub fn song_list_items<'a>(
    rows: &[Row],
    window_start: usize,
    visual_range: Option<(usize, usize)>,
    mut label: impl for<'s> FnMut(&'s Song) -> SongLabel<'s>,
    available_width: usize,
    ctx: SongListContext<'a>,
) -> Vec<ListItem<'a>> {
    let SongListContext {
        current,
        library,
        playlist_info,
    } = ctx;

    rows.iter()
        .enumerate()
        .map(|(local_index, row)| match row {
            Row::Header(heading) => ListItem::new(heading.clone()).style(header_style()),
            Row::Song(id, depth) => {
                let is_current = Some(*id) == current;
                let global_index = window_start + local_index;
                let is_visually_selected = visual_range
                    .is_some_and(|(start, end)| global_index >= start && global_index <= end);
                let row_background = is_visually_selected
                    .then(|| Style::new().bg(theme::visual_selection_background()));
                let indent = INDENT_UNIT.repeat(*depth);
                let marker = if is_current { NOW_PLAYING_MARKER } else { "  " };
                let prefix_width = display_width(&indent) + display_width(marker);
                let mut used = prefix_width;

                let mut spans: Vec<Span> = Vec::new();
                if !indent.is_empty() {
                    spans.push(Span::raw(indent));
                }
                spans.push(Span::styled(marker, now_playing_marker_style(is_current)));

                match library.get(*id) {
                    Some(song) => {
                        let SongLabel {
                            title,
                            artist,
                            detail,
                        } = label(song);

                        let text_budget = available_width.saturating_sub(prefix_width);
                        let separator_width = if artist.is_some() {
                            display_width(TITLE_ARTIST_SEPARATOR)
                        } else {
                            0
                        };
                        let widths = allocate_title_artist_widths(
                            text_budget.saturating_sub(separator_width),
                            display_width(title),
                            artist.map(display_width),
                        );

                        let title_text = marquee_window(title, widths.title_max);
                        used += display_width(&title_text);
                        spans.push(Span::styled(title_text, title_style(is_current)));

                        if let Some(artist) = artist {
                            used += separator_width;
                            spans.push(Span::styled(
                                TITLE_ARTIST_SEPARATOR,
                                separator_style(),
                            ));

                            let artist_max = widths.artist_max.unwrap_or(0);
                            let artist_text = marquee_window(artist, artist_max);
                            used += display_width(&artist_text);
                            spans.push(Span::styled(artist_text, artist_style()));
                        }

                        if let Some(detail) = detail {
                            let text = format!(" ({detail})");
                            used += display_width(&text);
                            spans.push(Span::styled(text, detail_style()));
                        }

                        if let Some((mode, lookup)) = playlist_info
                            && mode != PlaylistDisplayMode::Hidden
                        {
                            let names = lookup(*id);
                            let sep = " · ";
                            let remaining = available_width.saturating_sub(used + display_width(sep));
                            if let Some(suffix) = playlist_suffix(&names, mode, remaining) {
                                spans.push(Span::styled(sep, separator_style()));
                                spans.push(Span::styled(suffix, playlist_style()));
                            }
                        }
                    }
                    None => spans.push(Span::styled("<missing>", missing_style())),
                }

                let item = ListItem::new(Line::from(spans));
                match row_background {
                    Some(style) => item.style(style),
                    None => item,
                }
            }
        })
        .collect()
}

pub fn render_song_list_panel(
    area: Rect,
    buf: &mut Buffer,
    list_state: &mut ListState,
    rows: &[Row],
    opts: SongListPanelOptions<'_>,
    ctx: SongListContext<'_>,
    label: impl for<'s> FnMut(&'s Song) -> SongLabel<'s>,
) -> PanelHeight {
    let SongListPanelOptions {
        visual_range,
        title_prefix,
        category_label,
        sort_label,
        search_mode_open,
        query,
        group_label,
    } = opts;

    let match_count = song_count(rows);
    let border_style = if search_mode_open {
        focus_style()
    } else {
        unfocused_style()
    };
    let left_title = search_title(title_prefix, search_mode_open, query, match_count, border_style);
    let right_title = sort_title(category_label, sort_label, border_style);
    let block = titled_block_split(left_title, right_title, border_style);

    let inner_height = block.inner(area).height as usize;

    if match_count == 0 && is_filtering(query) {
        list_state.select(None);
        render_centered_message(area, buf, block, &format!("No songs match \"{query}\""));
        return PanelHeight(inner_height);
    }

    if match_count == 0 {
        render_centered_message(area, buf, block, "No songs here yet");
        return PanelHeight(inner_height);
    }

    let mut window = viewport(list_state, rows, inner_height);
    let pinned = pinned_group_label(rows, window.first, ctx.library, group_label);

    if pinned.is_some() {
        window = viewport(list_state, rows, inner_height.saturating_sub(1));
    }

    let inner = block.inner(area);
    let available_width =
        inner.width as usize - display_width(SELECTED_MARKER).min(inner.width as usize);

    let mut items = Vec::new();
    let mut selected_offset = 0;
    if let Some(text) = &pinned {
        let shown = marquee_window(text, available_width);
        items.push(ListItem::new(shown.into_owned()).style(header_style()));
        selected_offset = 1;
    }

    items.extend(song_list_items(
        window.items,
        window.first,
        visual_range,
        label,
        available_width,
        ctx,
    ));

    let mut local = ListState::default().with_offset(0);
    local.select(window.selected.map(|s| s + selected_offset));

    let list = styled_list(items, block);
    StatefulWidget::render(list, area, buf, &mut local);

    PanelHeight(inner_height)
}

pub fn render_no_matches(
    area: Rect,
    buf: &mut Buffer,
    block: Block<'static>,
    query: &str,
    noun: &str,
) {
    render_centered_message(area, buf, block, &format!("No {noun} match \"{query}\""));
}

pub fn render_centered_message(
    area: Rect,
    buf: &mut Buffer,
    block: Block<'static>,
    message: &str,
) {
    let inner = block.inner(area);
    Widget::render(block, area, buf);

    if inner.height == 0 {
        return;
    }

    let message_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1) / 2,
        width: inner.width,
        height: 1,
    };

    Paragraph::new(message.to_string())
        .style(
            Style::new()
                .fg(theme::empty_state())
                .add_modifier(Modifier::ITALIC),
        )
        .alignment(Alignment::Center)
        .render(message_area, buf);
}
