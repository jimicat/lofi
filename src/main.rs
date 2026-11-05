mod audio;
mod spectrum;
mod stations;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use rodio::{OutputStream, Sink};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use audio::{ConnState, Engine, OUT_RATE};
use stations::{Station, builtin_stations, custom_station};

const BANDS: usize = 48;

pub struct App {
    pub engine: Engine,
    sink: Sink,
    pub stations: Vec<Station>,
    pub current: usize,
    pub volume: u8,
    last_volume: u8,
    pub paused: bool,
    pub started: Instant,
    pub tick_count: u64,
    pub bands: Vec<f32>,
    pub peaks: Vec<f32>,
    pub visualizer: ui::Visualizer,
}

impl App {
    fn new(engine: Engine, sink: Sink, stations: Vec<Station>, start: usize, volume: u8) -> Self {
        let app = Self {
            engine,
            sink,
            stations,
            current: start,
            volume,
            last_volume: volume.max(1),
            paused: false,
            started: Instant::now(),
            tick_count: 0,
            bands: vec![0.0; BANDS],
            peaks: vec![0.0; BANDS],
            visualizer: ui::Visualizer::default(),
        };
        app.sink.set_volume(app.volume as f32 / 100.0);
        app.engine.tune(app.stations[start].clone());
        app
    }

    fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        let connection = self.engine.shared().connection();
        let active = !self.paused && connection.state == ConnState::Live;
        let target = if active {
            let samples = self
                .engine
                .shared()
                .analysis_snapshot(spectrum::ANALYSIS_WINDOW);
            spectrum::analyze(&samples, OUT_RATE, BANDS)
        } else {
            vec![0.0; BANDS]
        };
        for (index, value) in target.iter().enumerate() {
            let current = &mut self.bands[index];
            let response = if *value > *current { 0.55 } else { 0.12 };
            *current += (*value - *current) * response;
            let peak = &mut self.peaks[index];
            if *current > *peak {
                *peak = *current;
            } else {
                *peak = (*peak - 0.006).max(*current);
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char(' ') | KeyCode::Char('p') | KeyCode::Char('P') => self.toggle_pause(),
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Up | KeyCode::Char('k') => {
                self.change_volume(5)
            }
            KeyCode::Char('-') | KeyCode::Char('_') | KeyCode::Down | KeyCode::Char('j') => {
                self.change_volume(-5)
            }
            KeyCode::Right | KeyCode::Char('n') | KeyCode::Tab => self.change_station(1),
            KeyCode::Left | KeyCode::Char('b') | KeyCode::BackTab => self.change_station(-1),
            KeyCode::Char('m') | KeyCode::Char('M') => self.toggle_mute(),
            KeyCode::Char('r') | KeyCode::Char('R') => self.reconnect(),
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.visualizer = self.visualizer.next();
            }
            KeyCode::Char(digit @ '1'..='9') => {
                let index = (digit as u8 - b'1') as usize;
                if index < self.stations.len() && index != self.current {
                    self.select(index);
                }
            }
            _ => {}
        }
        false
    }

    fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if self.paused {
            self.sink.pause();
        } else {
            self.sink.play();
        }
    }

    fn change_volume(&mut self, delta: i16) {
        let volume = (self.volume as i16 + delta).clamp(0, 100) as u8;
        self.set_volume(volume, volume > 0);
    }

    fn set_volume(&mut self, volume: u8, remember: bool) {
        self.volume = volume;
        if remember {
            self.last_volume = volume;
        }
        self.sink.set_volume(volume as f32 / 100.0);
    }

    fn toggle_mute(&mut self) {
        if self.volume == 0 {
            let restored = if self.last_volume == 0 {
                60
            } else {
                self.last_volume
            };
            self.set_volume(restored, false);
        } else {
            let current = self.volume;
            self.set_volume(0, false);
            self.last_volume = current;
        }
    }

    fn change_station(&mut self, delta: isize) {
        let count = self.stations.len() as isize;
        let index = (self.current as isize + delta).rem_euclid(count) as usize;
        self.select(index);
    }

    fn select(&mut self, index: usize) {
        if index >= self.stations.len() {
            return;
        }
        self.current = index;
        self.engine.tune(self.stations[index].clone());
        self.bands.fill(0.0);
        self.peaks.fill(0.0);
        self.tick_count = 0;
        if self.paused {
            self.paused = false;
            self.sink.play();
        }
    }

    fn reconnect(&mut self) {
        self.engine.tune(self.stations[self.current].clone());
        self.bands.fill(0.0);
        self.peaks.fill(0.0);
        if self.paused {
            self.paused = false;
            self.sink.play();
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

fn main() {
    let args = match Args::parse() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n");
            print_usage();
            std::process::exit(2);
        }
    };
    if args.help {
        print_usage();
        return;
    }

    let mut stations = builtin_stations();
    let mut start = 0usize;
    if let Some(url) = args.url.clone() {
        stations.insert(0, custom_station(url, args.name.clone()));
        start = 0;
    }
    if let Some(selector) = args.station.as_deref() {
        match resolve_station(&stations, selector) {
            Some(index) => start = index,
            None => {
                eprintln!("找不到电台 \"{selector}\"");
                eprintln!("可用电台: {}", station_ids(&stations));
                std::process::exit(2);
            }
        }
    }
    if args.list {
        print_stations(&stations);
        return;
    }

    let (_stream, handle) = match OutputStream::try_default() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("无法打开系统音频输出设备: {error}");
            eprintln!("提示: 在 SSH、容器或无音频设备的环境中无法播放声音。");
            std::process::exit(1);
        }
    };
    let sink = match Sink::try_new(&handle) {
        Ok(sink) => sink,
        Err(error) => {
            eprintln!("无法创建音频播放器: {error}");
            std::process::exit(1);
        }
    };

    let engine = Engine::new();
    sink.append(engine.source());
    sink.play();
    let mut app = App::new(engine, sink, stations, start, args.volume);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    if let Err(error) = result {
        eprintln!("终端错误: {error}");
        std::process::exit(1);
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(32))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press && app.on_key(key) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

struct Args {
    station: Option<String>,
    url: Option<String>,
    name: Option<String>,
    volume: u8,
    list: bool,
    help: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = Self {
            station: None,
            url: None,
            name: None,
            volume: 60,
            list: false,
            help: false,
        };
        let mut values = std::env::args().skip(1);
        while let Some(argument) = values.next() {
            match argument.as_str() {
                "-s" | "--station" => {
                    args.station = Some(next_value(&mut values, &argument)?);
                }
                "-u" | "--url" => {
                    args.url = Some(next_value(&mut values, &argument)?);
                }
                "-n" | "--name" => {
                    args.name = Some(next_value(&mut values, &argument)?);
                }
                "-v" | "--volume" => {
                    let value = next_value(&mut values, &argument)?;
                    args.volume = value
                        .parse::<u8>()
                        .ok()
                        .filter(|volume| *volume <= 100)
                        .ok_or_else(|| format!("音量必须是 0-100 之间的整数，收到 \"{value}\""))?;
                }
                "-l" | "--list" => args.list = true,
                "-h" | "--help" => args.help = true,
                other => return Err(format!("未知参数 \"{other}\"")),
            }
        }
        if args.url.is_some() && !args.help && !args.list {
            let url = args.url.as_deref().unwrap();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("音频流地址必须以 http:// 或 https:// 开头".to_string());
            }
        }
        Ok(args)
    }
}

fn next_value(values: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    values.next().ok_or_else(|| format!("参数 {flag} 缺少取值"))
}

fn resolve_station(stations: &[Station], selector: &str) -> Option<usize> {
    if let Ok(number) = selector.parse::<usize>()
        && (1..=stations.len()).contains(&number)
    {
        return Some(number - 1);
    }
    let selector = selector.to_lowercase();
    stations.iter().position(|station| {
        station.id.to_lowercase() == selector || station.name.to_lowercase() == selector
    })
}

fn station_ids(stations: &[Station]) -> String {
    stations
        .iter()
        .map(|station| station.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_stations(stations: &[Station]) {
    println!("\n  终端 Lo-Fi 电台\n");
    for (index, station) in stations.iter().enumerate() {
        let bitrate = station
            .bitrate
            .map(|rate| format!("{rate} kbps"))
            .unwrap_or_else(|| "自定义流".to_string());
        println!(
            "  {:>2}  {}  {}  {}  {}",
            index + 1,
            pad(&station.id, 15),
            pad(&station.name, 23),
            pad(&station.tagline, 26),
            bitrate
        );
    }
    println!("\n  用法: lofi --station fluid\n");
}

fn pad(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let text_width = UnicodeWidthStr::width(text);
    if text_width <= width {
        let mut padded = text.to_string();
        padded.push_str(&" ".repeat(width - text_width));
        return padded;
    }
    let mut clipped = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width - 1 {
            break;
        }
        clipped.push(character);
        used += character_width;
    }
    clipped.push('…');
    used += 1;
    clipped.push_str(&" ".repeat(width - used));
    clipped
}

fn print_usage() {
    println!(
        "\
☕ LOFI RADIO — 终端里的 Lo-Fi 电台

用法:
  lofi [选项]

选项:
  -s, --station <ID>    启动时选择的电台 (见 --list，也可用序号)
  -u, --url <URL>       播放自定义 MP3 流
  -n, --name <NAME>     自定义流的显示名称
  -v, --volume <0-100>  初始音量 (默认 60)
  -l, --list            列出内置电台
  -h, --help            显示帮助

快捷键:
  Space / P      暂停或继续
  ← / →          上一个 / 下一个电台
  ↑ / ↓          调节音量
  M              静音
  R              重新连接
  V              切换频谱样式
  Q / Esc        退出
"
    );
}
