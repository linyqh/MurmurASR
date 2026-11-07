# Third-party licenses / 第三方许可说明

MurmurASR's own source code is released under the [MIT License](LICENSE).
That license covers the code in this repository only.

## Models

No model is bundled with the app or stored in this repository. Models are downloaded by the user
from the Models page, and **each model is used under its own license**, which can differ from MIT and
from other models. Some forbid commercial use. The license of every model is shown on its model card
in the app and in the `license` field of [`src-tauri/catalog.json`](src-tauri/catalog.json).
You are responsible for checking that a model's license allows your intended use (for example
commercial use, redistribution of its output, or sharing a modified model) before you use it.

| License | Models | Notes |
|---|---|---|
| Apache-2.0 | Qwen3-ASR 0.6B / 1.7B, Qwen3 forced aligner, Cohere Transcribe, Voxtral Mini 4B Realtime, Higgs Audio v3 STT, Granite Speech 5.0 TurboCTC, Whistle, QwenCleo-ASR, MOSS-Transcribe-Diarize, Index-Echo S2TT 2B / 9B, FireRedAudio | Permissive |
| MIT | Moonshine Tiny / Small / Medium, GigaAM v3 (CTC, RNN-T, E2E CTC, E2E RNN-T), GigaAM Multilingual (CTC, Large CTC), VibeVoice ASR (and Streaming 7B / 1.5B), Ming 3.0 Audio Tiny | Permissive |
| CC-BY-4.0 | OWSM v4 Base / Small / Medium, OWSM-CTC v4 1B, Citrinet 256, Parakeet TDT 0.6B v3, Canary 180M Flash | Attribution required |
| CC-BY-SA-4.0 | Kroko Community ASR (English, German, Spanish, French, Italian, Dutch, Portuguese, Swedish, Turkish, Hebrew) | Attribution and share-alike required |
| CC-BY-NC-4.0 | Hviske v5.3, Hviske v6 | **Non-commercial only** |
| NVIDIA non-commercial license | Audio Flamingo Next, Audio Flamingo 3 | **Non-commercial only** |
| Nyra Health non-commercial research license | CrisperWhisper 2.0 Large | **Non-commercial research only** |
| FunASR Model License v1.1 | SenseVoice Small, Fun-ASR Nano | Custom license, read it |
| ABR Open License v1.1 | Niagara 19M / 38M | Custom license, read it |
| LFM Open License v1.0 | LFM2.5-Audio 1.5B (English, Japanese) | Custom license, read it |
| NetEase Youdao model license | Confucius4-R2T2 | Custom license, read it |
| OpenMDW-1.1 | Nemotron 3.5 ASR 0.6B | Custom license, read it |

This table is a convenience summary generated from the catalog at the time of writing. The license
text published with each model is authoritative.

## Other components

- [audio.cpp](https://github.com/0xShug0/audio.cpp), the inference engine, is built from its own
  source and keeps its own license.
- ffmpeg is downloaded at runtime only when a file needs a codec the built-in decoder lacks, and is
  governed by its own license.
- JavaScript and Rust dependencies keep their respective licenses; see `package.json` and
  `src-tauri/Cargo.toml`.

---

## 中文说明

MurmurASR 自身的源代码以 [MIT 许可证](LICENSE) 发布，该许可证只覆盖本仓库里的代码。

安装包和仓库都不包含任何模型。模型由用户在「模型」页自行下载，**每个模型遵循它自己的许可证**，可能与 MIT 不同，模型之间也不一致，其中一部分禁止商用（上表中标为 Non-commercial 的）。每个模型的协议显示在应用内的模型卡片上，也记录在 [`src-tauri/catalog.json`](src-tauri/catalog.json) 的 `license` 字段。使用前请确认所选模型的协议允许你的使用场景（例如商用、分发其输出、分享修改后的模型）。上表只是方便查阅的摘要，以各模型随附的许可证原文为准。

audio.cpp 引擎、按需下载的 ffmpeg 以及各依赖库同样遵循各自的许可证。
