<p align="center">
  <img src="docs/assets/logo.png" width="96" alt="Meowcal Sub 猫咪图标">
</p>

<h1 align="center">Meowcal Sub</h1>

<p align="center">
  <strong>把屏幕上已有的字幕，翻译成你想读的语言。</strong><br>
  Windows 11 本地 OCR 与 AI 翻译。字幕文字留在你的电脑上。
</p>

<p align="center">
  <a href="https://github.com/PeterShanxin/Meowcal-Sub/releases/latest"><img alt="最新应用版本" src="https://img.shields.io/github/v/release/PeterShanxin/Meowcal-Sub?label=release"></a>
  <img alt="Windows 11，支持 x64 和 ARM64" src="https://img.shields.io/badge/Windows%2011-x64%20%7C%20ARM64-52627a">
  <a href="LICENSE"><img alt="许可证：AGPL-3.0-only" src="https://img.shields.io/badge/license-AGPL--3.0--only-52627a"></a>
</p>

<p align="center">
  <a href="#下载"><strong>下载 Windows 版</strong></a>
  &nbsp;·&nbsp; <a href="#开始使用">开始使用</a>
  &nbsp;·&nbsp; <a href="README.md">English</a>
  &nbsp;·&nbsp; <a href="https://github.com/PeterShanxin/Meowcal-Sub/issues/new/choose">反馈问题</a>
</p>

<p align="center">
  <img src="docs/assets/screenshot-home.png" width="640" alt="Meowcal Sub 主界面：准备把英文字幕翻译成简体中文，并显示 Start translation 按钮。">
</p>

圈出视频中的原字幕区域。Meowcal Sub 会识别这块区域的文字，在本机翻译，
再用悬浮字幕显示译文。正常使用不需要另外寻找字幕文件、注册云服务账号或填写 API key。

> **识别画面文字，不识别语音。** 视频需要有可见字幕。
> Meowcal Sub 不会听取音频，也不会为完全没有画面文字的视频生成字幕。

## 下载

**Windows 11 公开测试版 · v0.8.3** — [更新说明与全部安装文件](https://github.com/PeterShanxin/Meowcal-Sub/releases/tag/v0.8.3)。

| 你的电脑 | 安装包 |
| --- | --- |
| Intel / AMD Windows 电脑 | **[下载 x64（.exe）](https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.8.3/Meowcal.Sub_0.8.3_x64-setup.exe)** |
| 骁龙或其他 Windows on ARM 电脑 | **[下载 ARM64（.exe）](https://github.com/PeterShanxin/Meowcal-Sub/releases/download/v0.8.3/Meowcal.Sub_0.8.3_arm64-setup.exe)** |

MSI 安装包和 `SHA256SUMS.txt` 校验文件也在该发布页。
更新版本请看[最新应用发布页](https://github.com/PeterShanxin/Meowcal-Sub/releases/latest)。
使用安装版不需要自行安装 Rust、Node.js 或 Meowcal Core。

**引擎要求：** Windows 报告的系统内存至少 **8 GiB**，
引擎安装所在磁盘至少有 **3 GiB 空闲空间**。这是设置时的检查门槛，
不是流畅运行的保证；还要为视频播放器留出余量。

首次设置需要下载本地翻译运行环境与模型，约 **1.1 GB**。
缓存和保留的旧版本还需要额外磁盘空间。
Windows 也可能需要为原字幕语言安装对应的 OCR 识别组件。

**Windows 可能提示“未知发布者”。** 安装包尚未使用 Authenticode 签名。
请只从本仓库下载，并将文件的 SHA-256 与发布页校验文件比对；不要全局关闭 Windows 安全防护。
[安装与校验帮助（英文）→](docs/USAGE.md#installation)

## 开始使用

1. **设置翻译。** 打开应用并完成设置向导。选择原字幕语言和目标语言，
   让向导安装、测试翻译引擎，并检查 Windows 识别语言。
2. **圈选字幕。** 在**主显示器**播放视频，点击 **Select subtitle area**，
   圈住原字幕，而不是整个视频。保持该区域可见；在主显示器上移动视频或改变画面大小后，重新圈选。
3. **开始翻译。** 点击 **Start translation**，通过悬浮字幕阅读译文。
   在 **Subtitle style** 调整字号和深色／浅色底板；结束后点击 **Stop translation**。

![流程示意图：圈选已有字幕，使用 Windows OCR 识别，在本机通过 HY-MT 翻译，再显示悬浮字幕。](docs/assets/architecture.svg)

[设置、故障排查与常见问题（英文）→](docs/USAGE.md)

## 为观看而做

<p align="center">
  <img src="docs/assets/screenshot-overlay.png" width="720" alt="Meowcal Sub 演示画面：英文字幕 “The last ferry leaves before sunrise.”，下方悬浮字幕显示中文译文。">
</p>

| 你需要什么 | Meowcal Sub 怎么做 |
| --- | --- |
| 看懂画面里的字幕，不想再找字幕文件 | 用 Windows OCR 读取圈选区域。 |
| 不把识别内容和译文交给云服务 | OCR 与腾讯 HY-MT 翻译都在本机运行。 |
| 不离开视频就能阅读译文 | 始终置顶的悬浮字幕，支持调整字号与深色／浅色底板。 |
| 偶尔翻译字幕以外的可读文字 | 在 Settings 开启 **Translate any text**，放宽字幕专用筛选；默认关闭。 |
| 安装与维护本地引擎 | 设置向导、文件完整性校验、引擎修复，以及带签名验证的应用内更新。 |

**也有边界。** 默认的捕获／圈选流程面向**主显示器**，不支持将副显示器当作正常捕获目标。
识别效果取决于原文语言、文字清晰度，以及屏幕捕获实际能看到什么。
艺术字体、快速变化的文字或受保护的视频，可能造成漏识别或错误。
翻译并非瞬时完成，也不保证完全准确；速度取决于电脑和文本。

**GPU 支持因架构而异。** ARM64 的 Adreno 加速仅对已验证的硬件／驱动组合启用，
GPU 就绪检查超时后可尝试 CPU 回退。x64 版使用 Vulkan，当前**没有同样的验证门槛和应用级 CPU 重试**。
[GPU 兼容性说明（英文）→](docs/USAGE.md#how-does-gpu-support-differ-by-architecture)

## 隐私与联网

| 留在你的电脑上 | 需要联网的情况 |
| --- | --- |
| 圈选区域的画面捕获、Windows OCR、翻译推理和悬浮显示 | 引擎／模型安装或修复下载、必要的 Windows 识别语言安装，以及应用更新检查与下载 |

正常模式下，**识别的字幕文字与译文不会上传**。
生产日志记录支持代码、耗时和计数，不记录字幕文字。
安装好引擎和识别语言后，OCR 与翻译可以离线运行；在线视频本身仍可能需要联网。

应用启动时至多每天自动检查一次更新，也可以在
**Settings → Engine and updates → Check for updates** 手动检查。
应用更新只在你启动更新后下载。更新包签名验证与 Windows 发布者签名不是一回事，
不会消除前面提到的安装警告。

## More Meow tools

观看、理解和构建，各有一个小工具。猫猫不变，分工不同。

| 项目 | 用来做什么 |
| --- | --- |
| **[Meowcal Sub](https://github.com/PeterShanxin/Meowcal-Sub)** | 直接捕获屏幕字幕并在本地翻译，就是当前这个应用。 |
| **[Meowcal Sub 2](https://github.com/PeterShanxin/Meowcal-Sub-2)** | 搜索字幕，并让字幕会话跟随视频播放进度。 |
| **[MeowWatch](https://github.com/PeterShanxin/MeowWatch)** | 同步看视频，同时使用悬浮聊天。 |
| **[Meowcal Core](core/README.md)** | 面向开发者的共享、版本化 Windows OCR 与本地翻译运行环境；源码位于本仓库。 |

Sub 2 是另一种工作流，不是 Sub 1 用户必须升级到的版本。
Core 是底层运行环境，不是用户需要额外手动安装的应用。

## 技术实现

桌面应用采用 Tauri 2 与 Rust。[Meowcal Core](core/README.md) 负责原生 OCR 和
托管的 HY-MT 引擎；应用负责捕获、字幕筛选、翻译策略与显示。
本仓库发布 Windows x64 和 ARM64 安装包。

可阅读[架构说明](docs/ARCHITECTURE.md)、
[Core 设计决策](docs/adr/0004-versioned-meowcal-core.md)和
[Core 性能证据](docs/CORE_PERFORMANCE.md)。测量结果对应特定测试条件，
不是对所有电脑的端到端字幕延迟承诺。

## 反馈与参与

[反馈问题或提出改进建议](https://github.com/PeterShanxin/Meowcal-Sub/issues/new/choose)。
请提供应用版本、Windows 构建号、处理器架构和不含隐私的复现步骤。
不要公开贴出捕获的字幕文字、私人截图或含有这些内容的原始日志。
安全漏洞请按 [SECURITY.md](SECURITY.md) 私下报告。

开发请从 [CONTRIBUTING.md](CONTRIBUTING.md) 和
[开发约定](docs/AGENT_GUIDE.md) 开始。准备好 Windows 开发环境后运行：

```powershell
.\scripts\verify.ps1
.\dev-tauri.cmd
```

`.\dev-browser.cmd` 用于浏览器模式开发，不能证明原生捕获、OCR、悬浮层或安装包正常。
主动提交的贡献受 [CLA](CLA.md) 约束。

## 许可证

社区源码采用 **[AGPL-3.0-only](LICENSE)**，另见[应用声明](LICENSE-NOTICE.md)。
遵守 AGPL 使用公开项目无需购买许可证；需要不同条款的组织可采用商业许可。

可下载的腾讯 HY-MT 模型使用其独立的社区许可证，不属于应用的 AGPL 许可证。
[名称与图标的使用](TRADEMARKS.md)也单独约定。
