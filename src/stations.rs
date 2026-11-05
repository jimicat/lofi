use std::time::Duration;

pub const USER_AGENT: &str = "lofi-terminal/0.1 (+https://somafm.com)";

#[derive(Clone, Debug)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub url: String,
    pub somafm_id: Option<String>,
    pub bitrate: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub played_at: Option<u64>,
}

pub fn builtin_stations() -> Vec<Station> {
    vec![
        // 学习
        somafm("fluid", "Fluid", "学习 · 器乐嘻哈 · 慵懒", "fluid-128-mp3"),
        somafm(
            "groovesalad",
            "Groove Salad",
            "学习 · Downtempo 氛围",
            "groovesalad-128-mp3",
        ),
        somafm("lush", "Lush", "学习 · 梦幻人声 · 轻柔", "lush-128-mp3"),
        somafm(
            "bossa",
            "Bossa Beyond",
            "学习 · Bossa Nova · 桑巴",
            "bossa-128-mp3",
        ),
        somafm(
            "illstreet",
            "Illinois Street Lounge",
            "学习 · 复古 Lounge",
            "illstreet-128-mp3",
        ),
        direct(
            "swissjazz",
            "Radio Swiss Jazz",
            "学习 · 爵士标准曲",
            "https://stream.srg-ssr.ch/m/rsj/mp3_128",
        ),
        // 编程
        somafm(
            "beatblender",
            "Beat Blender",
            "编程 · Deep House · 深夜",
            "beatblender-128-mp3",
        ),
        somafm(
            "cliqhop",
            "Cliqhop IDM",
            "编程 · IDM · 故障电子",
            "cliqhop-128-mp3",
        ),
        somafm(
            "spacestation",
            "Space Station Soma",
            "编程 · 太空电子 · 中速",
            "spacestation-128-mp3",
        ),
        somafm(
            "defcon",
            "DEF CON Radio",
            "编程 · 黑客电子 · 极客",
            "defcon-128-mp3",
        ),
        // 睡眠
        somafm(
            "dronezone",
            "Drone Zone",
            "睡眠 · 深空氛围 · 长音",
            "dronezone-128-mp3",
        ),
        somafm(
            "deepspaceone",
            "Deep Space One",
            "睡眠 · 太空环境音乐",
            "deepspaceone-128-mp3",
        ),
        somafm(
            "synphaera",
            "Synphaera Radio",
            "睡眠 · 氛围 · 太空电子",
            "synphaera-128-mp3",
        ),
        somafm(
            "n5md",
            "n5MD Radio",
            "睡眠 · 氛围 · 现代古典",
            "n5md-128-mp3",
        ),
        // 游戏
        somafm(
            "secretagent",
            "Secret Agent",
            "游戏 · 复古间谍配乐",
            "secretagent-128-mp3",
        ),
        somafm(
            "vaporwaves",
            "Vaporwaves",
            "游戏 · Vaporwave · 复古",
            "vaporwaves-128-mp3",
        ),
        somafm(
            "u80s",
            "Underground 80s",
            "游戏 · 80 年代合成器",
            "u80s-128-mp3",
        ),
        somafm(
            "dubstep",
            "Dub Step Beyond",
            "游戏 · Dubstep · 深低音",
            "dubstep-128-mp3",
        ),
    ]
}

fn somafm(id: &str, name: &str, tagline: &str, stream: &str) -> Station {
    Station {
        id: id.to_string(),
        name: name.to_string(),
        tagline: tagline.to_string(),
        url: format!("https://ice2.somafm.com/{stream}"),
        somafm_id: Some(id.to_string()),
        bitrate: Some(128),
    }
}

fn direct(id: &str, name: &str, tagline: &str, url: &str) -> Station {
    Station {
        id: id.to_string(),
        name: name.to_string(),
        tagline: tagline.to_string(),
        url: url.to_string(),
        somafm_id: None,
        bitrate: Some(128),
    }
}

pub fn custom_station(url: String, name: Option<String>) -> Station {
    Station {
        id: "custom".to_string(),
        name: name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "自定义电台".to_string()),
        tagline: "自定义音频流".to_string(),
        url,
        somafm_id: None,
        bitrate: None,
    }
}

pub fn metadata_agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .user_agent(USER_AGENT)
        .build()
}

pub fn fetch_now_playing(agent: &ureq::Agent, somafm_id: &str) -> Result<Track, String> {
    let response = agent
        .get(&format!("https://somafm.com/songs/{somafm_id}.json"))
        .call()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value =
        serde_json::from_reader(response.into_reader()).map_err(|e| e.to_string())?;
    let song = json
        .get("songs")
        .and_then(|songs| songs.get(0))
        .ok_or_else(|| "播放列表为空".to_string())?;

    let text = |key: &str| {
        song.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let artist = text("artist");
    let title = text("title");
    if artist.is_empty() && title.is_empty() {
        return Err("播放列表为空".to_string());
    }
    let album = Some(text("album")).filter(|a| !a.is_empty());
    let played_at = song.get("date").and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    });

    Ok(Track {
        artist,
        title,
        album,
        played_at,
    })
}

impl Track {
    pub fn line(&self) -> String {
        match (self.artist.is_empty(), self.title.is_empty()) {
            (false, false) => format!("{} — {}", self.artist, self.title),
            (true, false) => self.title.clone(),
            (false, true) => self.artist.clone(),
            (true, true) => "未知曲目".to_string(),
        }
    }
}
