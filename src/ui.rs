use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::App;
use crate::audio::{ConnState, StreamInfo};

const ACCENT: Color = Color::Rgb(233, 170, 122);
const PINK: Color = Color::Rgb(228, 148, 176);
const PURPLE: Color = Color::Rgb(160, 142, 214);
const TEAL: Color = Color::Rgb(124, 198, 190);
const DIM: Color = Color::Rgb(112, 108, 134);
const TEXT: Color = Color::Rgb(228, 224, 240);
const BORDER: Color = Color::Rgb(58, 54, 78);
const SURFACE: Color = Color::Rgb(32, 30, 44);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Visualizer {
    #[default]
    Bars,
    Mirror,
    Dots,
    Wave,
    Stardust,
}

impl Visualizer {
    const ALL: [Visualizer; 5] = [
        Visualizer::Bars,
        Visualizer::Mirror,
        Visualizer::Dots,
        Visualizer::Wave,
        Visualizer::Stardust,
    ];

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|style| *style == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn title(self) -> &'static str {
        match self {
            Visualizer::Bars => " 频谱 · 柱状 ",
            Visualizer::Mirror => " 频谱 · 镜像 ",
            Visualizer::Dots => " 频谱 · 点阵 ",
            Visualizer::Wave => " 频谱 · 山形 ",
            Visualizer::Stardust => " 频谱 · 星尘 ",
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 56 || area.height < 18 {
        let message = vec![
            Line::from(Span::styled(
                "窗口太小",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "请放大终端窗口后继续收听",
                Style::default().fg(DIM),
            )),
        ];
        frame.render_widget(Paragraph::new(message).alignment(Alignment::Center), area);
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header, app);
    draw_footer(frame, footer);

    if body.width >= 84 {
        let [list, main] =
            Layout::horizontal([Constraint::Length(32), Constraint::Min(40)]).areas(body);
        draw_stations(frame, list, app);
        draw_main(frame, main, app);
    } else {
        draw_main(frame, body, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(26)]).areas(area);
    let mut title = vec![
        Span::styled("☕ ", Style::default().fg(ACCENT)),
        Span::styled(
            "LOFI RADIO",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 84 {
        title.push(Span::styled(
            "   lo-fi beats to code to",
            Style::default().fg(DIM),
        ));
    }
    let title = Line::from(title);
    frame.render_widget(Paragraph::new(title), left);
    let session = Line::from(Span::styled(
        format!("会话 {}", format_duration(app.elapsed().as_secs())),
        Style::default().fg(DIM),
    ));
    frame.render_widget(Paragraph::new(session).alignment(Alignment::Right), right);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let key = |label: &'static str| {
        Span::styled(
            label,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )
    };
    let label = |text: &'static str| Span::styled(text, Style::default().fg(DIM));
    let line = if area.width >= 100 {
        Line::from(vec![
            key("[Space]"),
            label(" 播放/暂停  "),
            key("[← →]"),
            label(" 换台  "),
            key("[↑ ↓]"),
            label(" 音量  "),
            key("[M]"),
            label(" 静音  "),
            key("[R]"),
            label(" 重连  "),
            key("[V]"),
            label(" 频谱  "),
            key("[Q]"),
            label(" 退出"),
        ])
    } else if area.width >= 76 {
        Line::from(vec![
            key("[Space]"),
            label(" 播放/暂停  "),
            key("[←→]"),
            label(" 换台  "),
            key("[↑↓]"),
            label(" 音量  "),
            key("[V]"),
            label(" 频谱  "),
            key("[Q]"),
            label(" 退出"),
        ])
    } else {
        Line::from(vec![
            key("Space"),
            label(" 暂停  "),
            key("←→"),
            label(" 换台  "),
            key("M"),
            label(" 静音  "),
            key("V"),
            label(" 频谱  "),
            key("Q"),
            label(" 退出"),
        ])
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn draw_stations(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel(" 电台 ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let stride: u16 = if inner.height >= app.stations.len() as u16 * 2 + 2 {
        2
    } else {
        1
    };
    let slots = (inner.height / stride).max(1) as usize;
    let offset = app.current.saturating_sub(slots.saturating_sub(1));
    for (index, station) in app.stations.iter().enumerate().skip(offset).take(slots) {
        let two_lines = stride == 2;
        let y = inner.y + (index - offset) as u16 * stride;
        let selected = index == app.current;
        let row_style = if selected {
            Style::default()
                .fg(Color::Rgb(24, 22, 32))
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let muted = if selected {
            row_style
        } else {
            Style::default().fg(DIM)
        };
        let marker = if selected { "▸" } else { " " };
        let prefix = format!(" {marker} {:>2}  ", index + 1);
        let available = inner
            .width
            .saturating_sub(UnicodeWidthStr::width(prefix.as_str()) as u16)
            as usize;
        let mut spans = vec![Span::styled(prefix, muted)];
        if two_lines {
            let name = if selected {
                marquee(&station.name, available, app.tick_count)
            } else {
                station.name.clone()
            };
            spans.push(Span::styled(name, row_style));
        } else if selected {
            let content = format!("{}  {}", station.name, station.tagline);
            spans.push(Span::styled(
                marquee(&content, available, app.tick_count),
                row_style,
            ));
        } else {
            spans.push(Span::styled(station.name.clone(), row_style));
            if inner.width > 28 {
                spans.push(Span::styled(
                    format!("  {}", station.tagline),
                    Style::default().fg(DIM),
                ));
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(if selected {
                Style::default().bg(ACCENT)
            } else {
                Style::default()
            }),
            Rect::new(inner.x, y, inner.width, 1),
        );
        if two_lines {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("      {}", station.tagline),
                    if selected {
                        row_style
                    } else {
                        Style::default().fg(DIM)
                    },
                )))
                .style(if selected {
                    Style::default().bg(ACCENT)
                } else {
                    Style::default()
                }),
                Rect::new(inner.x, y + 1, inner.width, 1),
            );
        }
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    let [now, spectrum_area, info] = Layout::vertical([
        Constraint::Length(8),
        Constraint::Min(3),
        Constraint::Length(4),
    ])
    .areas(area);

    draw_now_playing(frame, now, app);
    draw_spectrum(frame, spectrum_area, app);
    draw_info(frame, info, app);
}

fn draw_now_playing(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel(" 正在播放 ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let connection = app.engine.shared().connection();
    let [status_row, content] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let [status_left, status_right] =
        Layout::horizontal([Constraint::Min(12), Constraint::Length(22)]).areas(status_row);

    const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner_frame = SPINNER[(app.tick_count / 4) as usize % SPINNER.len()];
    let (badge, color) = if app.paused && connection.state == ConnState::Live {
        ("⏸ 已暂停".to_string(), ACCENT)
    } else {
        match connection.state {
            ConnState::Live => ("● LIVE".to_string(), TEAL),
            ConnState::Connecting => (format!("{spinner_frame} 连接中"), PURPLE),
            ConnState::Reconnecting => (format!("{spinner_frame} 重连中"), PINK),
        }
    };
    let station = &app.stations[app.current];
    let status = Line::from(vec![
        Span::styled(
            badge,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(BORDER)),
        Span::styled(station.name.clone(), Style::default().fg(TEXT)),
    ]);
    frame.render_widget(Paragraph::new(status), status_left);

    let fill = app.engine.shared().buffer_fill();
    let filled = (fill.clamp(0.0, 1.0) * 10.0).round() as usize;
    let buffer_color = if fill < 0.3 { PINK } else { TEAL };
    let buffer = Line::from(vec![
        Span::styled("缓冲 ", Style::default().fg(DIM)),
        Span::styled("█".repeat(filled), Style::default().fg(buffer_color)),
        Span::styled("░".repeat(10 - filled), Style::default().fg(BORDER)),
        Span::styled(format!(" {:>3.0}%", fill * 100.0), Style::default().fg(DIM)),
    ]);
    frame.render_widget(
        Paragraph::new(buffer).alignment(Alignment::Right),
        status_right,
    );

    let track = app.engine.shared().track();
    let mut lines: Vec<Line> = Vec::with_capacity(4);
    lines.push(Line::from(Span::styled(
        "正在播放",
        Style::default().fg(DIM),
    )));
    if let Some(track) = track {
        lines.push(Line::from(Span::styled(
            marquee(&track.line(), content.width as usize, app.tick_count),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        let mut meta = track.album.clone().unwrap_or_default();
        if let Some(age) = track_age(track.played_at) {
            if !meta.is_empty() {
                meta.push_str("  ·  ");
            }
            meta.push_str(&age);
        }
        lines.push(Line::from(Span::styled(meta, Style::default().fg(DIM))));
        lines.push(Line::from(Span::styled(
            station.tagline.clone(),
            Style::default().fg(DIM),
        )));
    } else {
        let has_metadata = station.somafm_id.is_some();
        lines.push(Line::from(Span::styled(
            if has_metadata {
                "~ 正在调频 ~"
            } else {
                "~ 实时直播 ~"
            },
            Style::default().fg(PURPLE).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            station.tagline.clone(),
            Style::default().fg(DIM),
        )));
        let note = connection.note.clone().unwrap_or_else(|| {
            if has_metadata {
                "SomaFM 实时直播 · 正在等待音频".to_string()
            } else {
                "实时直播流 · 该电台不提供曲目信息".to_string()
            }
        });
        lines.push(Line::from(Span::styled(
            truncate(&note, content.width as usize),
            Style::default().fg(DIM),
        )));
    }
    frame.render_widget(Paragraph::new(lines), content);
}

fn draw_spectrum(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel(app.visualizer.title());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let buffer = frame.buffer_mut();
    match app.visualizer {
        Visualizer::Bars => draw_bars(buffer, inner, &app.bands, &app.peaks),
        Visualizer::Mirror => draw_mirror(buffer, inner, &app.bands, &app.peaks),
        Visualizer::Dots => draw_dots(buffer, inner, &app.bands, &app.peaks),
        Visualizer::Wave => draw_wave(buffer, inner, &app.bands, &app.peaks),
        Visualizer::Stardust => {
            draw_stardust(buffer, inner, &app.bands, &app.peaks, app.tick_count)
        }
    }
}

fn draw_bars(buffer: &mut Buffer, area: Rect, bands: &[f32], peaks: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let rows = area.height as usize;
    let columns = area.width as usize;
    for column in 0..columns {
        let band = column * bands.len() / columns;
        let value = bands.get(band).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let eighths = (value * rows as f32 * 8.0).round() as usize;
        for row in 0..rows {
            let remaining = eighths.saturating_sub(row * 8).min(8);
            if remaining == 0 {
                break;
            }
            let y = area.y + area.height - 1 - row as u16;
            let level = (row as f32 + remaining as f32 / 8.0) / rows as f32;
            let color = spectrum_color(level, band as f32 / bands.len() as f32);
            buffer.set_string(
                area.x + column as u16,
                y,
                block_glyph(remaining as u8),
                Style::default().fg(color),
            );
        }
        draw_peak(
            buffer,
            area,
            column,
            value,
            peaks.get(band).copied().unwrap_or(0.0),
            rows,
        );
    }
}

fn draw_mirror(buffer: &mut Buffer, area: Rect, bands: &[f32], peaks: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let rows = area.height as usize;
    let columns = area.width as usize;
    let half = rows.div_ceil(2);
    for column in 0..columns {
        let band = column * bands.len() / columns;
        let value = bands.get(band).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let eighths = (value * half as f32 * 8.0).round() as usize;
        for row in 0..half {
            let remaining = eighths.saturating_sub(row * 8).min(8);
            if remaining == 0 {
                break;
            }
            let level = (row as f32 + remaining as f32 / 8.0) / half as f32;
            let color = spectrum_color(level, band as f32 / bands.len() as f32);
            let glyph = block_glyph(remaining as u8);
            let bottom = area.y + area.height - 1 - row as u16;
            buffer.set_string(
                area.x + column as u16,
                bottom,
                glyph,
                Style::default().fg(color),
            );
            let top = area.y + row as u16;
            if top != bottom {
                buffer.set_string(
                    area.x + column as u16,
                    top,
                    glyph,
                    Style::default().fg(color),
                );
            }
        }

        let peak = peaks.get(band).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let peak_eighths = (peak * half as f32 * 8.0).round() as usize;
        if peak_eighths > eighths + 2 {
            let row = peak_eighths.saturating_sub(1) / 8;
            if row < half {
                let bottom = area.y + area.height - 1 - row as u16;
                buffer.set_string(
                    area.x + column as u16,
                    bottom,
                    "▔",
                    Style::default().fg(PINK),
                );
                let top = area.y + row as u16;
                if top != bottom {
                    buffer.set_string(area.x + column as u16, top, "▁", Style::default().fg(PINK));
                }
            }
        }
    }
}

fn draw_dots(buffer: &mut Buffer, area: Rect, bands: &[f32], peaks: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let rows = area.height as usize;
    let columns = area.width as usize;
    let grid = Style::default().fg(Color::Rgb(44, 40, 58));
    for row in 0..rows {
        let y = area.y + area.height - 1 - row as u16;
        for column in 0..columns {
            buffer.set_string(area.x + column as u16, y, "·", grid);
        }
    }
    for column in 0..columns {
        let band = column * bands.len() / columns;
        let value = bands.get(band).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let filled = (value * rows as f32).round() as usize;
        for row in 0..filled.min(rows) {
            let y = area.y + area.height - 1 - row as u16;
            let level = (row as f32 + 0.5) / rows as f32;
            let color = spectrum_color(level, band as f32 / bands.len() as f32);
            let glyph = if row + 1 == filled { "◦" } else { "•" };
            buffer.set_string(area.x + column as u16, y, glyph, Style::default().fg(color));
        }
        draw_peak(
            buffer,
            area,
            column,
            value,
            peaks.get(band).copied().unwrap_or(0.0),
            rows,
        );
    }
}

fn draw_wave(buffer: &mut Buffer, area: Rect, bands: &[f32], peaks: &[f32]) {
    if bands.is_empty() {
        return;
    }
    let rows = area.height as usize;
    let columns = area.width as usize;
    for column in 0..columns {
        let value = smooth_band(bands, column, columns);
        let total = (value * rows as f32 * 8.0).round() as usize;
        let full = total / 8;
        let partial = total % 8;
        let position = column as f32 / columns as f32;
        for row in 0..full.min(rows) {
            let y = area.y + area.height - 1 - row as u16;
            let level = (row as f32 + 0.5) / rows as f32;
            let color = if row + 1 == full {
                spectrum_color(level, position)
            } else {
                mix(Color::Rgb(42, 52, 64), Color::Rgb(62, 48, 80), level)
            };
            buffer.set_string(area.x + column as u16, y, "█", Style::default().fg(color));
        }
        if partial > 0 && full < rows {
            let y = area.y + area.height - 1 - full as u16;
            let level = (full as f32 + partial as f32 / 8.0) / rows as f32;
            buffer.set_string(
                area.x + column as u16,
                y,
                block_glyph(partial as u8),
                Style::default().fg(spectrum_color(level, position)),
            );
        }
        draw_peak(
            buffer,
            area,
            column,
            value,
            smooth_band(peaks, column, columns),
            rows,
        );
    }
}

fn draw_stardust(buffer: &mut Buffer, area: Rect, bands: &[f32], peaks: &[f32], tick: u64) {
    if bands.is_empty() {
        return;
    }
    let rows = area.height as usize;
    let columns = area.width as usize;
    let era = tick / 6;
    for column in 0..columns {
        let value = interpolate_band(bands, column, columns);
        let envelope = (value * rows as f32).round() as usize;
        let position = column as f32 / columns as f32;
        for row in 0..envelope.min(rows) {
            let noise = dust_hash(column, row, era);
            let density = 30 + (value * 40.0) as u32;
            if noise % 100 >= density {
                continue;
            }
            let level = (row as f32 + 0.5) / rows as f32;
            let glyph = if noise.is_multiple_of(23) {
                "•"
            } else {
                "·"
            };
            buffer.set_string(
                area.x + column as u16,
                area.y + area.height - 1 - row as u16,
                glyph,
                Style::default().fg(spectrum_color(level, position)),
            );
        }
        if envelope > 0 {
            let row = (era as usize + column * 5) % envelope;
            let level = (row as f32 + 0.5) / rows as f32;
            buffer.set_string(
                area.x + column as u16,
                area.y + area.height - 1 - row as u16,
                "•",
                Style::default().fg(mix(PINK, spectrum_color(level, position), 0.35)),
            );
        }
        draw_peak(
            buffer,
            area,
            column,
            value,
            interpolate_band(peaks, column, columns),
            rows,
        );
    }
}

fn draw_peak(buffer: &mut Buffer, area: Rect, column: usize, value: f32, peak: f32, rows: usize) {
    let eighths = (value * rows as f32 * 8.0).round() as usize;
    let peak_eighths = (peak * rows as f32 * 8.0).round() as usize;
    if peak_eighths <= eighths + 2 {
        return;
    }
    let row = peak_eighths.saturating_sub(1) / 8;
    if row >= rows {
        return;
    }
    let y = area.y + area.height - 1 - row as u16;
    buffer.set_string(area.x + column as u16, y, "▔", Style::default().fg(PINK));
}

fn interpolate_band(values: &[f32], column: usize, columns: usize) -> f32 {
    if values.is_empty() || columns == 0 {
        return 0.0;
    }
    let position = column as f32 / columns as f32 * (values.len() - 1) as f32;
    let low = position.floor() as usize;
    let high = (low + 1).min(values.len() - 1);
    let fraction = position - low as f32;
    values[low].clamp(0.0, 1.0) * (1.0 - fraction) + values[high].clamp(0.0, 1.0) * fraction
}

fn smooth_band(values: &[f32], column: usize, columns: usize) -> f32 {
    let left = interpolate_band(values, column.saturating_sub(1), columns);
    let center = interpolate_band(values, column, columns);
    let right = interpolate_band(values, (column + 1).min(columns.saturating_sub(1)), columns);
    (left + center * 2.0 + right) / 4.0
}

fn dust_hash(column: usize, row: usize, era: u64) -> u32 {
    let mut value = (column as u32 + 1)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add((row as u32 + 1).wrapping_mul(0x85EB_CA6B))
        .wrapping_add((era as u32 + 1).wrapping_mul(0xC2B2_AE35));
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^ (value >> 15)
}

fn draw_info(frame: &mut Frame, area: Rect, app: &App) {
    let [volume_area, audio_area] =
        Layout::horizontal([Constraint::Length(34), Constraint::Min(24)]).areas(area);

    let gauge_color = if app.volume == 0 { DIM } else { ACCENT };
    let label = if app.volume == 0 {
        " 静音 ".to_string()
    } else {
        format!(" {}% ", app.volume)
    };
    let gauge = Gauge::default()
        .block(panel(" 音量 "))
        .gauge_style(
            Style::default()
                .fg(gauge_color)
                .bg(Color::Rgb(46, 42, 60))
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.volume as f64 / 100.0)
        .label(label);
    frame.render_widget(gauge, volume_area);

    let connection = app.engine.shared().connection();
    let info = app.engine.shared().stream_info();
    let first = match info {
        Some(info) => stream_summary(&info),
        None => "等待音频数据…".to_string(),
    };
    let second = match connection.note {
        Some(note) if connection.state != ConnState::Live => {
            format!("{} · 第 {} 次尝试", note, connection.attempt.max(1))
        }
        _ => format!(
            "缓冲 {:.0}% · 已收听 {}",
            app.engine.shared().buffer_fill() * 100.0,
            format_duration(app.elapsed().as_secs())
        ),
    };
    let lines = vec![
        Line::from(Span::styled(first, Style::default().fg(TEXT))),
        Line::from(Span::styled(
            truncate(&second, audio_area.width.saturating_sub(2) as usize),
            Style::default().fg(DIM),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).block(panel(" 音频 ")), audio_area);
}

fn stream_summary(info: &StreamInfo) -> String {
    let channels = match info.channels {
        1 => "单声道".to_string(),
        2 => "立体声".to_string(),
        count => format!("{count} 声道"),
    };
    let bitrate = info
        .bitrate
        .map(|rate| format!("{rate} kbps"))
        .unwrap_or_else(|| "未知码率".to_string());
    format!(
        "{} · {:.1} kHz · {} · {}",
        info.codec,
        info.sample_rate as f32 / 1000.0,
        channels,
        bitrate
    )
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(Line::from(Span::styled(title, Style::default().fg(DIM))))
        .style(Style::default().bg(SURFACE))
}

fn format_duration(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

fn track_age(played_at: Option<u64>) -> Option<String> {
    let played_at = played_at?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let delta = now.saturating_sub(played_at);
    Some(if delta < 60 {
        "刚刚".to_string()
    } else if delta < 3600 {
        format!("{} 分钟前", delta / 60)
    } else {
        format!("{} 小时前", delta / 3600)
    })
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let mut output = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width.saturating_sub(1) {
            output.push('…');
            break;
        }
        output.push(character);
        used += character_width;
    }
    output
}

fn marquee(text: &str, width: usize, tick: u64) -> String {
    if width == 0 {
        return String::new();
    }
    let text_width = UnicodeWidthStr::width(text);
    if text_width <= width {
        return text.to_string();
    }
    let gap = 7usize;
    let cycle = text_width + gap;
    let offset = (tick as usize / 4) % cycle;
    let padded = format!("{text}{}", " ".repeat(gap));
    let mut output = String::new();
    let mut skipped = 0usize;
    let mut used = 0usize;
    for character in padded.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if skipped + character_width <= offset {
            skipped += character_width;
            continue;
        }
        if used + character_width > width {
            break;
        }
        output.push(character);
        used += character_width;
    }
    output
}

fn block_glyph(eighths: u8) -> &'static str {
    const GLYPHS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    GLYPHS[eighths.clamp(1, 8) as usize - 1]
}

fn spectrum_color(level: f32, position: f32) -> Color {
    let level = (level + (position - 0.5) * 0.12).clamp(0.0, 1.0);
    if level < 0.42 {
        mix(TEAL, PURPLE, level / 0.42)
    } else if level < 0.74 {
        mix(PURPLE, PINK, (level - 0.42) / 0.32)
    } else {
        mix(PINK, ACCENT, (level - 0.74) / 0.26)
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let (Color::Rgb(fr, fg, fb), Color::Rgb(tr, tg, tb)) = (from, to) else {
        return to;
    };
    let blend =
        |start: u8, end: u8| (start as f32 + (end as f32 - start as f32) * amount).round() as u8;
    Color::Rgb(blend(fr, tr), blend(fg, tg), blend(fb, tb))
}
