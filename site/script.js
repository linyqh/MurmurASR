// Tiny client-side i18n: English is the markup default; Chinese overrides by data-i18n key.
const zh = {
  "nav.features": "功能", "nav.models": "模型", "nav.api": "API", "nav.download": "下载",
  "hero.eyebrow": "版本 0.1.14 · macOS 与 Windows",
  "hero.sub": "语音转文字，音频始终留在你的电脑上。",
  "hero.lead": "55 个开源语音识别模型按需下载。文件转写、实时听写、字幕导出，也能被脚本和 AI Agent 调用。不用云服务，不用账号，不按时长计费。",
  "hero.download": "免费下载", "hero.github": "在 GitHub 查看",
  "stats.models": "个开源模型", "stats.speed": "Apple 芯片上的实时倍速", "stats.languages": "种语言与方言", "stats.upload": "字节被上传",
  "features.title": "你需要的都有，数据不出这台电脑",
  "features.offline.t": "本地且离线", "features.offline.d": "推理在本机完成，音频不会上传，也不需要注册账号。",
  "features.gpu.t": "GPU 加速", "features.gpu.d": "基于 audio.cpp。Apple 芯片用 Metal，Windows / Linux 用 Vulkan 或 CUDA，没有 GPU 时自动改用 CPU。",
  "features.live.t": "文件转写与实时听写", "features.live.d": "拖入音频或视频，边识别边看到文字；也可以对着麦克风实时听写。",
  "features.subs.t": "好读的字幕", "features.subs.d": "词级时间戳，按停顿、标点和阅读速度自动断行，可导出 SRT、VTT、TXT、Markdown 或 JSON。",
  "features.i18n.t": "English 与简体中文", "features.i18n.d": "界面跟随系统语言，新增语言只需要添加一个 JSON 文件夹。",
  "features.agent.t": "为 Agent 而生", "features.agent.d": "本机 HTTP API 和现成的 Agent Skill，脚本、其他设备和 AI 助手都能提交任务。",
  "models.title": "挑一个适合你的模型",
  "models.lead": "从 40 MB 的英语小模型到 7B 的长音频大模型：Qwen3-ASR、SenseVoice、Parakeet、OWSM、Moonshine 等。按需下载，支持断点续传、SHA-256 校验和镜像下载源。",
  "models.a": "覆盖中文、粤语、英语、欧洲语言和小语种", "models.b": "有流式、词级时间戳和说话人区分的模型", "models.c": "Hugging Face、ModelScope 和 HF 镜像三种下载源",
  "api.title": "一次请求就能转写", "api.lead": "应用运行时会提供本机 HTTP 接口，与界面共用同一个推理队列。",
  "final.title": "获取 MurmurASR", "final.lead": "免费开源，MIT 许可证。macOS 版本已通过 Apple 签名与公证。",
  "final.download": "下载最新版本", "final.docs": "阅读文档", "footer.powered": "技术支持："
};
const nodes = [...document.querySelectorAll("[data-i18n]")];
const en = Object.fromEntries(nodes.map((n) => [n.dataset.i18n, n.innerHTML]));
const button = document.getElementById("lang");

function apply(lang) {
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const n of nodes) n.innerHTML = lang === "zh" ? (zh[n.dataset.i18n] ?? en[n.dataset.i18n]) : en[n.dataset.i18n];
  button.textContent = lang === "zh" ? "English" : "中文";
  button.dataset.lang = lang;
  document.title = lang === "zh" ? "MurmurASR — 离线语音转文字" : "MurmurASR — Offline speech to text";
  try { localStorage.setItem("lang", lang); } catch {}
}

let saved = null;
try { saved = localStorage.getItem("lang"); } catch {}
apply(saved ?? (navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en"));
button.addEventListener("click", () => apply(button.dataset.lang === "zh" ? "en" : "zh"));
