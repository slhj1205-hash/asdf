use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::{Color, palette::tailwind};
use serde::Deserialize;

#[derive(Clone, Copy)]
pub struct Theme {
    pub title: Color,
    pub title_current: Color,
    pub artist: Color,
    pub detail: Color,
    pub separator: Color,
    pub playlist_tag: Color,
    pub now_playing_marker_idle: Color,
    pub missing_song: Color,
    pub section_header: Color,
    pub empty_state: Color,
    pub playlist_name: Color,
    pub playlist_song_count: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_dim: Color,
    pub status_info: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub focus: Color,
    pub key_hint: Color,
    pub highlight: Color,
    pub modal_background: Color,
    pub selected_background: Color,
    pub visual_selection_background: Color,
    pub dim_foreground: Color,
    pub dim_background: Color,
    pub gauge_foreground: Color,
    pub gauge_background: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            title: tailwind::SLATE.c100,
            title_current: tailwind::CYAN.c300,
            artist: tailwind::ORANGE.c200,
            detail: tailwind::SLATE.c600,
            separator: tailwind::SLATE.c600,
            playlist_tag: tailwind::VIOLET.c300,
            now_playing_marker_idle: tailwind::SLATE.c600,
            missing_song: tailwind::SLATE.c500,
            section_header: tailwind::SLATE.c400,
            empty_state: tailwind::SLATE.c500,
            playlist_name: tailwind::SLATE.c100,
            playlist_song_count: tailwind::SKY.c300,
            text_primary: tailwind::SLATE.c100,
            text_secondary: tailwind::SLATE.c200,
            text_muted: tailwind::SLATE.c400,
            text_dim: tailwind::SLATE.c500,
            status_info: tailwind::SLATE.c300,
            success: tailwind::GREEN.c400,
            warning: tailwind::AMBER.c400,
            error: tailwind::RED.c400,
            focus: tailwind::YELLOW.c400,
            key_hint: tailwind::CYAN.c300,
            highlight: tailwind::CYAN.c400,
            modal_background: tailwind::SLATE.c900,
            selected_background: tailwind::SLATE.c800,
            visual_selection_background: tailwind::CYAN.c950,
            dim_foreground: tailwind::SLATE.c700,
            dim_background: tailwind::SLATE.c950,
            gauge_foreground: Color::White,
            gauge_background: Color::Black,
        }
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();

pub fn init_from_path(path: &Path) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    match parse(&contents) {
        Ok(theme) => {
            THEME.set(theme).ok();
        }
        Err(message) => {
            eprintln!(
                "warning: ignoring {}: {message}",
                path.display()
            );
        }
    }
}

pub fn parse(source: &str) -> Result<Theme, String> {
    let file: ThemeFile =
        toml::from_str(source).map_err(|e| e.message().to_string())?;
    let mut theme = Theme::default();
    apply_overrides(source, file, &mut theme)?;
    Ok(theme)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    title: Option<toml::Spanned<String>>,
    title_current: Option<toml::Spanned<String>>,
    artist: Option<toml::Spanned<String>>,
    detail: Option<toml::Spanned<String>>,
    separator: Option<toml::Spanned<String>>,
    playlist_tag: Option<toml::Spanned<String>>,
    now_playing_marker_idle: Option<toml::Spanned<String>>,
    missing_song: Option<toml::Spanned<String>>,
    section_header: Option<toml::Spanned<String>>,
    empty_state: Option<toml::Spanned<String>>,
    playlist_name: Option<toml::Spanned<String>>,
    playlist_song_count: Option<toml::Spanned<String>>,
    text_primary: Option<toml::Spanned<String>>,
    text_secondary: Option<toml::Spanned<String>>,
    text_muted: Option<toml::Spanned<String>>,
    text_dim: Option<toml::Spanned<String>>,
    status_info: Option<toml::Spanned<String>>,
    success: Option<toml::Spanned<String>>,
    warning: Option<toml::Spanned<String>>,
    error: Option<toml::Spanned<String>>,
    focus: Option<toml::Spanned<String>>,
    key_hint: Option<toml::Spanned<String>>,
    highlight: Option<toml::Spanned<String>>,
    modal_background: Option<toml::Spanned<String>>,
    selected_background: Option<toml::Spanned<String>>,
    visual_selection_background: Option<toml::Spanned<String>>,
    dim_foreground: Option<toml::Spanned<String>>,
    dim_background: Option<toml::Spanned<String>>,
    gauge_foreground: Option<toml::Spanned<String>>,
    gauge_background: Option<toml::Spanned<String>>,
}

fn apply_overrides(source: &str, file: ThemeFile, theme: &mut Theme) -> Result<(), String> {
    override_color(source, "title", file.title, |t, c| t.title = c, theme)?;
    override_color(source, "title_current", file.title_current, |t, c| t.title_current = c, theme)?;
    override_color(source, "artist", file.artist, |t, c| t.artist = c, theme)?;
    override_color(source, "detail", file.detail, |t, c| t.detail = c, theme)?;
    override_color(source, "separator", file.separator, |t, c| t.separator = c, theme)?;
    override_color(source, "playlist_tag", file.playlist_tag, |t, c| t.playlist_tag = c, theme)?;
    override_color(source, "now_playing_marker_idle", file.now_playing_marker_idle, |t, c| t.now_playing_marker_idle = c, theme)?;
    override_color(source, "missing_song", file.missing_song, |t, c| t.missing_song = c, theme)?;
    override_color(source, "section_header", file.section_header, |t, c| t.section_header = c, theme)?;
    override_color(source, "empty_state", file.empty_state, |t, c| t.empty_state = c, theme)?;
    override_color(source, "playlist_name", file.playlist_name, |t, c| t.playlist_name = c, theme)?;
    override_color(source, "playlist_song_count", file.playlist_song_count, |t, c| t.playlist_song_count = c, theme)?;
    override_color(source, "text_primary", file.text_primary, |t, c| t.text_primary = c, theme)?;
    override_color(source, "text_secondary", file.text_secondary, |t, c| t.text_secondary = c, theme)?;
    override_color(source, "text_muted", file.text_muted, |t, c| t.text_muted = c, theme)?;
    override_color(source, "text_dim", file.text_dim, |t, c| t.text_dim = c, theme)?;
    override_color(source, "status_info", file.status_info, |t, c| t.status_info = c, theme)?;
    override_color(source, "success", file.success, |t, c| t.success = c, theme)?;
    override_color(source, "warning", file.warning, |t, c| t.warning = c, theme)?;
    override_color(source, "error", file.error, |t, c| t.error = c, theme)?;
    override_color(source, "focus", file.focus, |t, c| t.focus = c, theme)?;
    override_color(source, "key_hint", file.key_hint, |t, c| t.key_hint = c, theme)?;
    override_color(source, "highlight", file.highlight, |t, c| t.highlight = c, theme)?;
    override_color(source, "modal_background", file.modal_background, |t, c| t.modal_background = c, theme)?;
    override_color(source, "selected_background", file.selected_background, |t, c| t.selected_background = c, theme)?;
    override_color(source, "visual_selection_background", file.visual_selection_background, |t, c| t.visual_selection_background = c, theme)?;
    override_color(source, "dim_foreground", file.dim_foreground, |t, c| t.dim_foreground = c, theme)?;
    override_color(source, "dim_background", file.dim_background, |t, c| t.dim_background = c, theme)?;
    override_color(source, "gauge_foreground", file.gauge_foreground, |t, c| t.gauge_foreground = c, theme)?;
    override_color(source, "gauge_background", file.gauge_background, |t, c| t.gauge_background = c, theme)?;
    Ok(())
}

fn override_color(
    source: &str,
    name: &str,
    value: Option<toml::Spanned<String>>,
    assign: impl FnOnce(&mut Theme, Color),
    theme: &mut Theme,
) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let color = parse_hex_color(value.get_ref()).ok_or_else(|| {
        format!(
            "line {}: '{}' must be \"#rgb\" or \"#rrggbb\", got {}",
            line_of(source, value.span().start),
            name,
            value.get_ref(),
        )
    })?;
    assign(theme, color);
    Ok(())
}

fn line_of(source: &str, byte_offset: usize) -> usize {
    source.bytes().take(byte_offset).filter(|&b| b == b'\n').count() + 1
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let digits = value.strip_prefix('#')?;
    match digits.len() {
        3 => {
            let mut channels = digits.chars();
            Some(Color::Rgb(
                hex_digit(&mut channels)? * 17,
                hex_digit(&mut channels)? * 17,
                hex_digit(&mut channels)? * 17,
            ))
        }
        6 => {
            let mut digits = digits.chars();
            Some(Color::Rgb(
                hex_channel(&mut digits)?,
                hex_channel(&mut digits)?,
                hex_channel(&mut digits)?,
            ))
        }
        _ => None,
    }
}

fn hex_digit(digits: &mut dyn Iterator<Item = char>) -> Option<u8> {
    digits.next()?.to_digit(16).map(|d| d as u8)
}

fn hex_channel(digits: &mut dyn Iterator<Item = char>) -> Option<u8> {
    let high = digits.next()?.to_digit(16)? as u8;
    let low = digits.next()?.to_digit(16)? as u8;
    Some(high * 16 + low)
}

fn current() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

pub fn title() -> Color {
    current().title
}

pub fn title_current() -> Color {
    current().title_current
}

pub fn artist() -> Color {
    current().artist
}

pub fn detail() -> Color {
    current().detail
}

pub fn separator() -> Color {
    current().separator
}

pub fn playlist_tag() -> Color {
    current().playlist_tag
}

pub fn now_playing_marker_idle() -> Color {
    current().now_playing_marker_idle
}

pub fn missing_song() -> Color {
    current().missing_song
}

pub fn section_header() -> Color {
    current().section_header
}

pub fn empty_state() -> Color {
    current().empty_state
}

pub fn playlist_name() -> Color {
    current().playlist_name
}

pub fn playlist_song_count() -> Color {
    current().playlist_song_count
}

pub fn text_primary() -> Color {
    current().text_primary
}

pub fn text_secondary() -> Color {
    current().text_secondary
}

pub fn text_muted() -> Color {
    current().text_muted
}

pub fn text_dim() -> Color {
    current().text_dim
}

pub fn status_info() -> Color {
    current().status_info
}

pub fn success() -> Color {
    current().success
}

pub fn warning() -> Color {
    current().warning
}

pub fn error() -> Color {
    current().error
}

pub fn focus() -> Color {
    current().focus
}

pub fn key_hint() -> Color {
    current().key_hint
}

pub fn highlight() -> Color {
    current().highlight
}

pub fn modal_background() -> Color {
    current().modal_background
}

pub fn selected_background() -> Color {
    current().selected_background
}

pub fn visual_selection_background() -> Color {
    current().visual_selection_background
}

pub fn dim_foreground() -> Color {
    current().dim_foreground
}

pub fn dim_background() -> Color {
    current().dim_background
}

pub fn gauge_foreground() -> Color {
    current().gauge_foreground
}

pub fn gauge_background() -> Color {
    current().gauge_background
}
