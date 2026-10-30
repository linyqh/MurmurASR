# Murmur

本地语音转文字桌面客户端。推理核心是 [audio.cpp](https://github.com/0xShug0/audio.cpp)（基于 ggml），界面用 [Tauri 2](https://tauri.app) 构建，风格贴近 macOS 26/27 原生应用（Liquid Glass）。所有识别都在本机完成，音频不会离开设备。

- **文件转写**：拖入音频或视频文件（MP3、M4A、WAV、FLAC、OGG、MP4、MOV 等）即可转写，进度实时显示。内置解码器不支持的编码（如 HE-AAC、AC-3、WMA）会在首次遇到时自动下载 ffmpeg（约 20–30 MB，SHA-256 校验，存放在数据目录 `tools/` 下）来解码；下载源跟随模型下载源设置，在 GitHub 与 npmmirror 之间切换
- **实时听写**：用麦克风实时转文字，说话过程中显示中间结果，可选择保存录音
- **记录**：可搜索、编辑、跟随播放定位，支持导出 TXT / SRT / VTT / Markdown / JSON
- **API 接口**：App 运行时在 7895 端口提供 HTTP 接口，本机和局域网内的其他设备、Agent 都可以调用转写（附带 Agent Skill）
- **模型管理**：安装包不含模型，首次启动会引导下载推荐模型；支持断点续传和 SHA-256 校验，下载源可选 Hugging Face、ModelScope 或 HF 镜像

## 模型

模型页收录了 audio.cpp（v0.9.3）中可以直接下载使用的语音识别模型，共 55 个，另有 1 个对齐模型。按用途分为 7 组，可以按名称、语言或特点搜索。每张卡片都写了模型出处、擅长的场景、限制、内存需求和许可证。安装包里不带模型，需要哪个就下载哪个。

| 分组 | 代表模型 |
|---|---|
| 通用多语种（含中文） | **Qwen3-ASR 0.6B（推荐）**、Qwen3-ASR 1.7B、SenseVoice Small、Fun-ASR Nano、Confucius4-R2T2、Nemotron 3.5、Cohere Transcribe、Voxtral Realtime、CrisperWhisper、OWSM / OWSM-CTC |
| 英语 | Moonshine Tiny / Small / Medium、Kroko、Niagara、Citrinet、Higgs Audio STT |
| 欧洲语言 | Parakeet TDT、Canary 180M Flash、Granite Speech 5.0、Whistle、Kroko 各语种版 |
| 特定语言优化 | GigaAM（俄语及中亚语言）、Hviske（丹麦语）、QwenCleo（埃及阿拉伯语）、Kroko 希伯来语 |
| 长音频与说话人区分 | MOSS-Transcribe-Diarize、VibeVoice ASR、VibeVoice ASR Streaming 7B / 1.5B |
| 语音翻译 | Index-Echo S2TT 2B / 9B（中文语音直接输出英 / 西 / 日文字） |
| 音频理解大模型（实验性） | Audio Flamingo 3 / Next、Ming 3.0 Audio Tiny、FireRedAudio、LFM2.5-Audio |

模型目录在 `src-tauri/catalog.json`，后端和前端共用这一份。每个模型由一个或多个文件组成。单文件模型直接加载这个文件，多文件模型（如 LFM2.5-Audio）加载所在目录。每个文件都有 SHA-256，Hugging Face 链接固定到具体 commit。某个下载源返回的文件校验不通过时，会自动换下一个源。`engine/audiocpp.lock` 的 `AUDIOCPP_MODELS` 必须包含目录中用到的每个 family，单元测试会检查这一点。

未收录的模型：audio8_asr（许可证不允许公开分发 GGUF）、lfm3_audio（只在 audio.cpp main 分支上，v0.9.3 没有）、SAMSONE（音频描述模型，即使要求转写也只输出对声音的描述）、VibeASR（由两个 GGUF 组成，上游要求额外传入 model spec 才能加载）。

### 词级时间戳

支持的模型会自动请求词级时间戳。记录页播放时逐词高亮，点击单词可跳转播放；导出 JSON 时每段带 `words`。

- Parakeet、Kroko、GigaAM、Granite Speech、CrisperWhisper、Nemotron 原生输出词级时间戳。Nemotron 给出的是子词 token，会合并成词。
- Qwen3-ASR 需要额外下载 **Qwen3 强制对齐模型**（1.1 GB，在模型卡片里下载），支持中、粤、英、日、韩、法、德、意、葡、俄、西 11 种语言，中文按字对齐。下载后重新加载模型即可生效。对齐失败的片段会自动退回为不带时间戳的结果。
- 实时听写只为最终结果生成时间戳，中间结果不做对齐。
- 编辑某段文字后，该段的词级时间戳会被清除。

### 字幕编排

文件和保存后的实时记录会使用独立的字幕编排模块：结合词级时间、标点、停顿、阅读速度与行长选择字幕边界，默认生成一行字幕，可在设置或字幕预览中切换为最多两行。切换行数会重新选择分段边界，并记住全局偏好。支持跨识别块组合短片段，明显停顿处保留空白。通用预设以每行约 42 个英文字符 / 16 个汉字、每条最长 7 秒为目标；过快或无法合理拆分的内容会提示检查，不会删改识别文本。

记录页默认显示“字幕预览”，换行与 SRT / VTT 导出一致；切换“原始转写 · 编辑”可查看及修改原文。已有记录打开后自动重新编排，无需重新识别。JSON 的 `segments` 保留原始转写和词级时间，新增 `subtitles` 保存编排结果，HTTP API 使用相同格式。

缺少、失效或不完整的词级对齐会保留原段起止时间并标注提示，只调整行内换行，不按字数伪造细分时间。手动改字后，该段也会使用此保守处理。

实测（Apple M6，Metal）：Qwen3-ASR 0.6B 转写 7 分钟英文约 21 秒（约 20× 实时）；SenseVoice 转写 31 秒中文约 0.6 秒。新安装后首次加载模型需要编译 Metal 着色器（约 7 秒），之后约 1 秒。

## API 接口与 Agent Skill

App 运行时会启动 HTTP 服务（默认端口 7895）。侧边栏的 **API 接口** 页面可以开关服务、开关局域网访问、修改端口，并显示本机 / 局域网地址、接口列表和 curl / Python / JavaScript 调用示例。通过接口提交的任务和界面里的任务共用同一个推理队列，结果同样保存到记录中。

局域网访问默认开启，同一网络中的其他电脑、手机可以用 `http://<本机局域网 IP>:7895` 调用。接口没有密码，在公共网络中建议关闭局域网访问。

```bash
curl http://127.0.0.1:7895/health
# 本机调用：直接传路径，不上传
curl -H 'Content-Type: application/json' -d '{"path":"/abs/path/talk.m4a","format":"srt"}' http://127.0.0.1:7895/transcribe
# 局域网内其他设备：上传文件
curl -F file=@talk.m4a -F format=txt http://192.168.1.23:7895/transcribe
```

| 接口 | 说明 |
|---|---|
| `GET /health` | 引擎、已加载模型、可用语言、已安装模型 |
| `GET /models` / `POST /models/load` | 模型列表 / 加载已下载的模型 |
| `POST /transcribe` | 转写（JSON 传 `path` 或 multipart 传 `file`；`format` 可选 json/txt/srt/vtt/md；`wait=false` 异步） |
| `GET /jobs/{id}` / `POST /jobs/{id}/cancel` | 查询进度 / 取消 |
| `GET /transcripts` / `GET /transcripts/{id}?format=` | 历史记录（仅本机） / 按格式导出 |

服务启动后会把地址写入数据目录下的 `api.json`，供本机调用方发现端口，App 退出时删除。为防止网页借浏览器调用，接口拒绝带 `Origin` 头、或 `Host` 为其他域名的请求。局域网内的其他设备只能上传文件，不能用 `path` 读取本机文件，也不能列出历史记录。

`skills/murmur-asr/` 是给 Agent 用的 skill（`SKILL.md` + 完整接口说明 + 零依赖的 Python 调用脚本），打包时会放进 App 资源目录，设置页可直接定位。把这个文件夹复制到 Agent 的 skills 目录（如 `~/.claude/skills/`）即可。

## 架构

```
React UI ──invoke/events──▶ Rust (Tauri)
                              ├─ commands.rs   命令、状态、历史持久化
                              ├─ api.rs        本地 HTTP 接口（axum）
                              ├─ models.rs     模型目录、下载（续传 / 校验 / 多源）
                              ├─ audio.rs      symphonia 解码、重采样、cpal 麦克风采集
                              ├─ ffmpeg.rs     symphonia 不支持时的 ffmpeg 解码（按需下载）
                              └─ engine/       专用推理线程
                                   ├─ ffi.rs   audiocpp.h 的安全封装（运行时 dlopen）
                                   └─ vad.rs   Silero VAD 分段 / 实时断句
                                        │
                                        ▼
                              libaudiocpp（audio.cpp C ABI，Metal / Vulkan / CUDA / CPU）
```

文件和实时两条路径都先用 Silero VAD 切分语音，再逐段离线识别。这样所有模型（包括不支持流式的）都能得到进度、可取消，并带字幕时间戳。实时模式下，说话过程中会定期重新识别当前语句作为中间结果，检测到停顿后再输出最终文字。

## 开发

需要 Rust、Node 22、CMake；macOS 还需要 Xcode。

```bash
npm install
npm run engine:build      # 拉取锁定版本的 audio.cpp 并编译 libaudiocpp（macOS 默认 Metal）
npm run app:dev           # 启动开发版
```

不启动 Tauri、只在浏览器里预览界面：运行 `npm run dev` 后打开 `http://localhost:1420/?mock=home`。可用的场景有 `onboarding`、`onboarding-dl`、`models`、`jobs`、`live`；用 `#transcript:t1`、`#settings` 这样的锚点可以直接打开对应页面。

### 测试

```bash
npm test && npm run typecheck
cd src-tauri && cargo test

# 用真实引擎和模型的端到端测试（没有模型时自动跳过）
MURMUR_TEST_MODEL=/path/qwen3-asr-0.6b-q8_0.gguf MURMUR_TEST_AUDIO=/path/speech.m4a \
  cargo test --release --test engine -- --nocapture
# 加上 MURMUR_TEST_ALIGNER=/path/qwen3-forced-aligner-0.6b-q8_0.gguf 会同时校验词级时间戳

# 下载测试：会下载约 254 MB，测试中途取消后续传并校验 SHA-256
MURMUR_TEST_DOWNLOAD=1 cargo test --release --test download
```

### 打包

```bash
./scripts/package-macos.sh   # 生成 .app 和 .dmg
```

Windows（在 PowerShell 中运行，生成 NSIS 安装程序）：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1              # Vulkan（默认）
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1 -Backend cpu -RebuildEngine
```

产物为 `src-tauri/target/release/bundle/nsis/Murmur_<版本>_x64-setup.exe` 及同名 `.sha256`。
需要 Visual Studio 2022（C++ 工作负载，脚本会自动使用其自带的 CMake）、Rust、Node 22、Git，
Vulkan 后端还需要 Vulkan SDK（`VULKAN_SDK` 环境变量，或安装在 `C:\VulkanSDK\` 下）。
安装程序按当前用户安装、无需管理员权限，内置 WebView2 引导程序；引擎目录随附 VC++ 运行库与
Vulkan 加载器，因此目标机器无需另装 VC++ 运行库，没有 Vulkan 驱动时自动使用 CPU 推理。

DMG 包含冰蓝安装引导背景、固定的拖拽图标位置，以及首次下载模型的说明。
脚本首次运行会用 Python 3 在 `src-tauri/target/dmg-tools` 创建隔离环境，安装锁定版本的
`dmgbuild` 依赖；直接生成 Finder 布局，无需 AppleScript 或自动化权限。
背景素材与生成提示词位于 `assets/dmg/`，布局配置位于 `scripts/dmg-settings.py`。

macOS 本机签名：复制 `.env.macos.local.example` 为 `.env.macos.local`，将
`APPLE_SIGNING_IDENTITY` 设置为 `security find-identity -v -p codesigning` 中的
Developer ID Application 身份。打包脚本会加载此本机配置，对应用和 DMG 签名并验证；
`.env.macos.local` 不纳入 Git。

需要 Apple 公证时，先运行 `xcrun notarytool store-credentials murmur-notary`，
按提示将凭据保存到钥匙串，再在本机配置中设置 `MURMUR_NOTARY_PROFILE="murmur-notary"`。
脚本会依次公证应用和 DMG，并附加、验证公证票据。未配置该项时只构建签名包，不完成公证。

## 跨平台

| 平台 | 推理后端 | 引擎构建 | 状态 |
|---|---|---|---|
| macOS（Apple 芯片） | Metal | `engine/scripts/build-engine.sh` | 已在本机构建并验证 |
| Windows x64 | Vulkan / CUDA / CPU | `engine/scripts/build-engine.ps1` | 已在本机构建并验证（Vulkan，无驱动时退回 CPU） |
| Linux x64 | Vulkan / CPU | `engine/scripts/build-engine.sh --backend vulkan` | 已有脚本和 CI，尚未在真机上验证 |

`.github/workflows/build.yml` 会在三个平台上分别编译引擎、运行测试并打包。窗口外观按平台区分：macOS 用 sidebar 材质，Windows 用 Mica，Linux 用不透明窗口（见 `tauri.*.conf.json`）。

audio.cpp 的版本锁定在 `engine/audiocpp.lock`。升级时修改 commit，再重新运行引擎构建脚本。模型权重遵循各自的原始许可证（见模型卡片）。
