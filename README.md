# ☕ LOFI RADIO

一个用 Rust 编写的终端 Lo-Fi 网络电台。直接流式播放公开的 MP3 电台，在 TUI 中显示正在播放的曲目、实时频谱和缓冲状态，全部操作都可以在键盘上完成。

## 特性

- **18 个内置电台**，按学习、编程、睡眠、游戏四种场景分组
- **真正的流式播放**：边下载边解码，4 秒有界缓冲，网络波动不会卡住界面
- **断流自动重连**：1/2/4/8 秒指数退避，界面实时显示连接状态
- **正在播放信息**：SomaFM 电台自动获取曲目、专辑和播放时间
- **5 种频谱样式**：柱状、镜像、点阵、山形、星尘，按 `V` 循环切换
- **自定义电台**：通过 `--url` 播放任意公开 MP3 流
- **自适应布局**：从 56×18 的小窗口到宽屏都能正常显示，支持中文宽字符对齐
- **不阻塞的交互**：换台、暂停、重连在缓冲或网络等待时也能立即响应

## 安装

需要可用的音频输出设备、网络连接；从源码构建需要 Rust 1.85+。

### Homebrew

```bash
brew tap jimicat/lofi https://github.com/jimicat/lofi
brew install jimicat/lofi/lofi
lofi
```

Formula 会从固定版本的源码编译，并自动处理 Rust 构建依赖；Linux 上会同时安装 `alsa-lib`。使用 `brew install --HEAD jimicat/lofi/lofi` 可以安装 `main` 分支的最新代码。

### 预编译产物

推送 `v*` 标签后，GitHub Actions 会自动构建 macOS（Apple Silicon / Intel）和 Linux x86_64 的压缩包，并生成 `SHA256SUMS`，随 GitHub Release 一起发布：

```bash
tar -xzf lofi-0.1.0-aarch64-apple-darwin.tar.gz
./lofi-0.1.0-aarch64-apple-darwin/lofi
```

Linux 产物需要系统安装 ALSA 运行库（Debian/Ubuntu：`sudo apt install libasound2`）。

### 从源码运行

```bash
git clone https://github.com/jimicat/lofi.git
cd lofi
cargo run --release
```

安装到 `~/.cargo/bin`：

```bash
cargo install --path .
lofi
```

## 使用

```bash
lofi                          # 启动默认电台 Fluid
lofi --list                   # 列出所有内置电台
lofi --station defcon         # 按 ID 选择电台
lofi --station 7              # 按序号选择电台
lofi --volume 40              # 初始音量 40%
lofi --url https://example.com/stream.mp3 --name "我的电台"
```

| 选项 | 说明 |
|---|---|
| `-s, --station <ID>` | 启动时选择的电台，可用 ID、名称或序号 |
| `-u, --url <URL>` | 播放自定义 MP3 流，需以 `http://` 或 `https://` 开头 |
| `-n, --name <NAME>` | 自定义流的显示名称 |
| `-v, --volume <0-100>` | 初始音量，默认 60 |
| `-l, --list` | 列出内置电台 |
| `-h, --help` | 显示帮助 |

## 快捷键

| 按键 | 作用 |
|---|---|
| `Space` / `P` | 暂停 / 继续 |
| `+` `=` `↑` `K` / `-` `_` `↓` `J` | 音量 ±5 |
| `→` `N` `Tab` / `←` `B` | 下一个 / 上一个电台 |
| `1`–`9` | 直接切换到对应电台 |
| `M` | 静音 / 恢复音量 |
| `R` | 强制重新连接当前电台 |
| `V` | 切换频谱样式 |
| `Q` / `Esc` / `Ctrl+C` | 退出 |

## 内置电台

| 场景 | 电台 | ID | 风格 |
|---|---|---|---|
| 学习 | Fluid | `fluid` | 器乐嘻哈 · 慵懒节拍 |
| 学习 | Groove Salad | `groovesalad` | Downtempo 氛围电子 |
| 学习 | Lush | `lush` | 梦幻人声 · 轻柔电子 |
| 学习 | Bossa Beyond | `bossa` | Bossa Nova · 桑巴 |
| 学习 | Illinois Street Lounge | `illstreet` | 复古 Lounge |
| 学习 | Radio Swiss Jazz | `swissjazz` | 爵士标准曲 |
| 编程 | Beat Blender | `beatblender` | Deep House · 深夜律动 |
| 编程 | Cliqhop IDM | `cliqhop` | IDM · 故障电子 |
| 编程 | Space Station Soma | `spacestation` | 太空电子 · 中速 |
| 编程 | DEF CON Radio | `defcon` | 黑客电子 · 极客氛围 |
| 睡眠 | Drone Zone | `dronezone` | 深空氛围 · 极简长音 |
| 睡眠 | Deep Space One | `deepspaceone` | 太空环境音乐 |
| 睡眠 | Synphaera Radio | `synphaera` | 氛围 · 太空电子 |
| 睡眠 | n5MD Radio | `n5md` | 氛围 · 现代古典 |
| 游戏 | Secret Agent | `secretagent` | 复古间谍片配乐 |
| 游戏 | Vaporwaves | `vaporwaves` | Vaporwave · 复古电子 |
| 游戏 | Underground 80s | `u80s` | 80 年代合成器流行 |
| 游戏 | Dub Step Beyond | `dubstep` | Dubstep · 深低音 |

其中 SomaFM 电台提供实时曲目元数据；Radio Swiss Jazz 为 48 kHz 直连流，会自动重采样到 44.1 kHz。

## 音频管线

```
HTTP 流 (ureq，可中断读取)
  → Symphonia MP3 解码
  → 重采样 / 声道转换
  → 4 秒有界采样队列
  → rodio Sink 输出
```

- 解码和网络都在后台线程，UI 主线程不会被阻塞。
- 队列满时解码线程等待，避免无限内存增长。
- 每个调台请求带一个 generation，旧连接和旧缓冲会被立即失效，因此换台不需要等网络超时。
- 读取使用 10 秒读超时，断流后自动重连并保留用户操作。
- 频谱由 2048 点 FFT 计算，映射到 48 个对数频段，再做时间平滑和峰值保持。

## 项目结构

```
src/
├── main.rs       参数解析、应用状态、事件循环和按键处理
├── audio.rs      HTTP 流读取、MP3 解码、缓冲队列、重连与元数据线程
├── stations.rs   内置电台、自定义电台、SomaFM 曲目接口
├── spectrum.rs   FFT 与频段分析
└── ui.rs         ratatui 界面、5 种频谱绘制、布局与滚动
```

## 开发

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# 需要网络和音频设备
cargo test -- --ignored
```

## 发布

```bash
git tag v0.1.0
git push origin v0.1.0
```

[release.yml](.github/workflows/release.yml) 会构建三个平台的产物、生成校验和并创建 GitHub Release。发布后需要同步更新 [Formula/lofi.rb](Formula/lofi.rb) 中的 `url`、`sha256` 和 `version`，让 Homebrew 安装新的版本。

## 说明与限制

- 目前只支持 MP3 流；AAC、Opus、HLS 等格式尚未接入。
- SomaFM 电台需要自定义 User-Agent，程序已内置处理。
- 没有音频设备的环境（例如部分容器或 CI）无法启动播放。
- 暂停时音频输出停止，但网络和解码线程仍会维持当前缓冲。
- 所有电台均为第三方公开流，可用性和内容由对应电台提供方决定。

## 致谢

- [SomaFM](https://somafm.com/) 提供的大部分电台与实时曲目数据
- [Radio Swiss Jazz](https://www.radioswissjazz.ch/) 提供的爵士直播流
- [ratatui](https://ratatui.rs/)、[rodio](https://github.com/RustAudio/rodio)、[Symphonia](https://github.com/pdeljanov/Symphonia)、[ureq](https://github.com/algesten/ureq)
