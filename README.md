# Murmur

本地语音转文字桌面客户端。推理核心是 [audio.cpp](https://github.com/0xShug0/audio.cpp)（基于 ggml），界面用 [Tauri 2](https://tauri.app) 构建，风格贴近 macOS 26/27 原生应用（Liquid Glass）。所有识别都在本机完成，音频不会离开设备。

- **文件转写**：拖入音频或视频文件（MP3、M4A、WAV、FLAC、OGG、MP4、MOV 等）即可转写，进度实时显示
- **实时听写**：用麦克风实时转文字，说话过程中显示中间结果，可选择保存录音
- **记录**：可搜索、编辑、跟随播放定位，支持导出 TXT / SRT / VTT / Markdown / JSON
- **模型管理**：安装包不含模型，首次启动会引导下载推荐模型；支持断点续传和 SHA-256 校验，下载源可选 Hugging Face、ModelScope 或 HF 镜像

## 模型

| 模型 | 大小 | 特点 |
|---|---|---|
| **Qwen3-ASR 0.6B（推荐）** | 1.2 GB | 30 种语言 + 22 种中文方言，自动识别语言 |
| SenseVoice Small | 254 MB | 极速轻量，中文 / 粤语出色 |
| Fun-ASR Nano | 1.0 GB | 中文专精，数字规整好 |
| Qwen3-ASR 1.7B | 2.5 GB | 更高精度 |
| Parakeet TDT 0.6B v3 | 916 MB | 英语和 24 种欧洲语言，逐词时间戳 |
| Nemotron 3.5 ASR 0.6B | 931 MB | 40 种语言 |
| Moonshine Small | 301 MB | 英语，低延迟 |

模型目录在 `src-tauri/catalog.json`，后端和前端共用这一份。

实测（Apple M6，Metal）：Qwen3-ASR 0.6B 转写 7 分钟英文约 21 秒（约 20× 实时）；SenseVoice 转写 31 秒中文约 0.6 秒。新安装后首次加载模型需要编译 Metal 着色器（约 7 秒），之后约 1 秒。

## 架构

```
React UI ──invoke/events──▶ Rust (Tauri)
                              ├─ commands.rs   命令、状态、历史持久化
                              ├─ models.rs     模型目录、下载（续传 / 校验 / 多源）
                              ├─ audio.rs      symphonia 解码、重采样、cpal 麦克风采集
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

# 下载测试：会下载约 254 MB，测试中途取消后续传并校验 SHA-256
MURMUR_TEST_DOWNLOAD=1 cargo test --release --test download
```

### 打包

```bash
./scripts/package-macos.sh   # 生成 .app 和 .dmg
```

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
| Windows x64 | Vulkan / CUDA / CPU | `engine/scripts/build-engine.ps1` | 已有脚本和 CI，尚未在真机上验证 |
| Linux x64 | Vulkan / CPU | `engine/scripts/build-engine.sh --backend vulkan` | 已有脚本和 CI，尚未在真机上验证 |

`.github/workflows/build.yml` 会在三个平台上分别编译引擎、运行测试并打包。窗口外观按平台区分：macOS 用 sidebar 材质，Windows 用 Mica，Linux 用不透明窗口（见 `tauri.*.conf.json`）。

audio.cpp 的版本锁定在 `engine/audiocpp.lock`。升级时修改 commit，再重新运行引擎构建脚本。模型权重遵循各自的原始许可证（见模型卡片）。
