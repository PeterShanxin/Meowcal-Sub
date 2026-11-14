// 产品官网的简体中文文案，结构与 content.en.mjs 一一对应。

const SUB1_REPO = "https://github.com/PeterShanxin/Meowcal-Sub";
const SUB2_REPO = "https://github.com/PeterShanxin/Meowcal-Sub-2";

export default {
  lang: "zh-CN",
  ogLocale: "zh_CN",
  languageName: "简体中文",
  skipLink: "跳到正文",
  navLabel: "产品",
  languageLabel: "语言",
  source: "GitHub",
  downloadLabel: "下载 Windows 版",
  howLabel: "了解使用方法",
  footer: {
    links: [
      { label: "Meowcal Sub 源码", href: SUB1_REPO },
      { label: "Meowcal Sub 2 源码", href: SUB2_REPO },
      { label: "名称与图标的使用", href: `${SUB1_REPO}/blob/main/TRADEMARKS.md` },
      { label: "反馈问题", href: "@issues" },
    ],
  },

  sub1: {
    title: "Meowcal Sub · 妙影 – Windows 11 本地屏幕字幕翻译",
    description:
      "Meowcal Sub · 妙影识别屏幕上已有的字幕，在你的电脑上完成 AI 翻译，并悬浮显示译文。免费开源，支持 Windows 11 x64 与 ARM64，无需账号或 API 密钥。",
    name: "Meowcal Sub · 妙影",
    navName: "Meowcal Sub · 妙影",
    tagline: "把屏幕上已有的字幕，翻译成你想读的语言。",
    promise: "全本地运行 · 保护隐私 · 无付费",
    lead: "圈出视频中的原字幕区域。Meowcal Sub 会识别这块区域的文字，在本机翻译，再用悬浮字幕显示译文。正常使用不需要另找字幕文件、注册云服务账号或填写 API 密钥。",
    actions: {
      x64: { label: "下载 x64 版", hint: "Intel / AMD 电脑" },
      arm64: { label: "下载 ARM64 版", hint: "骁龙或其他 Windows on ARM 电脑" },
      store: { label: "从 Microsoft Store 获取" },
      meta: "版本 {version} · Windows 11 公开测试版 · 以 AGPL-3.0-only 协议免费开源",
      links: [
        { label: "更新说明与全部安装文件", href: "{notes}" },
        { label: "使用指南", href: `${SUB1_REPO}/blob/main/docs/USAGE.zh-CN.md` },
        { label: "源代码", href: `${SUB1_REPO}/blob/main/README.zh-CN.md` },
      ],
    },
    figure: {
      image: "sub1-overlay",
      alt: "影片画面中，英文字幕 “The sea is quiet. We still have time.” 位于圈选区域内，下方的悬浮字幕显示中文译文“海面很平静，我们还有时间。”",
      caption: "截图展示应用界面，其中的字幕为编写的示例。",
    },
    sections: [
      {
        type: "callout",
        title: "识别画面文字，不识别语音。",
        body: "视频需要有可见字幕。Meowcal Sub 不会听取音频，也不会为完全没有画面文字的视频生成字幕。",
      },
      {
        type: "steps",
        id: "how-it-works",
        heading: "使用方法",
        items: [
          {
            title: "设置翻译",
            body: "打开应用，按设置向导完成设置。向导会下载并测试本地翻译引擎。完成后，选择原字幕语言和想阅读的语言。",
            image: "sub1-home",
            alt: "Meowcal Sub 首页：“原字幕”和“翻译为”两个语言列表，以及“选择字幕区域”“使用已保存区域开始”两个按钮。",
          },
          {
            title: "选择字幕区域",
            body: "将应用主窗口移到播放视频的屏幕，点击 **选择字幕区域**。圈住原字幕，而不是整个视频，再点击 **确认并开始**。",
            image: "sub1-selection",
            alt: "字幕区域选择界面：一个方框圈住字幕，下方是“确认并开始”等按钮。",
          },
          {
            title: "观看",
            body: "通过悬浮字幕阅读译文。在 **字幕样式** 调整文字大小和深色／浅色背景。结束后点击 **退出翻译** 或 **停止翻译**。",
            image: "sub1-style",
            alt: "字幕样式页面：实时预览、文字大小滑块，以及深色／浅色背景选项。",
          },
        ],
      },
      {
        type: "pairs",
        id: "features",
        heading: "为观看而做",
        termLabel: "你需要什么",
        detailLabel: "Meowcal Sub 怎么做",
        items: [
          {
            term: "看懂画面里的字幕，不想再找字幕文件",
            detail: "用内置的 PP-OCRv6 读取圈选区域，也可以改用 Windows OCR。",
          },
          {
            term: "不把识别内容和译文交给云服务",
            detail: "OCR 与腾讯 HY-MT 翻译都在你的电脑上运行。",
          },
          {
            term: "不离开视频就能阅读译文",
            detail: "始终置顶的悬浮字幕，支持调整文字大小与深色／浅色背景。",
          },
          {
            term: "在常看的语言之间翻译",
            detail:
              "原字幕和译文语言可选：简体中文、繁体中文、日语、韩语、英语、西班牙语、法语和德语。",
          },
          {
            term: "偶尔翻译字幕以外的可读文字",
            detail: "在设置中开启 **翻译任意文字**，放宽字幕专用筛选。该选项默认关闭。",
          },
          {
            term: "安装与维护本地引擎",
            detail: "设置向导、文件完整性校验、引擎修复，以及带签名验证的应用内更新。",
          },
        ],
      },
      {
        type: "columns",
        id: "privacy",
        heading: "隐私与联网",
        columns: [
          {
            title: "留在你的电脑上",
            items: ["圈选区域的画面捕获", "文字识别（OCR）", "翻译推理", "悬浮字幕显示"],
          },
          {
            title: "需要联网的情况",
            items: [
              "设置或修复时下载引擎与模型",
              "必要时安装 Windows 识别语言",
              "应用更新检查与下载",
            ],
          },
        ],
        body: "正常模式下，识别的字幕文字与译文不会上传。安装好引擎和识别语言后，OCR 与翻译可以离线运行。在线视频本身仍可能需要联网。",
      },
      {
        type: "columns",
        id: "requirements",
        heading: "系统要求与边界",
        columns: [
          {
            title: "你需要准备",
            items: [
              "Windows 11，x64 或 ARM64。",
              "Windows 报告的系统内存至少 8 GiB，引擎安装所在磁盘至少有 3 GiB 空闲空间。这是设置时的检查门槛，不是流畅运行的保证。",
              "首次设置需要联网，下载本地翻译运行环境与模型，约 1.1 GB。",
              "识别韩语字幕，或选择 Windows OCR 时：需要为原字幕语言安装对应的 Windows 识别语言。内置的 PP-OCRv6 不需要 Windows 语言组件。",
            ],
          },
          {
            title: "也有边界",
            items: [
              "每次只翻译一个屏幕，以圈选字幕区域时应用所在的屏幕为准。",
              "字幕区域是固定的屏幕区域。移动视频或改变画面大小后，需要重新圈选。",
              "识别效果取决于原文语言、文字清晰度，以及屏幕捕获实际能看到什么。艺术字体、快速变化的文字或受保护的视频，可能造成漏识别或错误。",
              "翻译并非瞬时完成，也不保证完全准确。速度取决于电脑和文本。",
            ],
          },
        ],
        body: `**Windows 可能提示“未知发布者”。** GitHub 上的安装包尚未使用 Authenticode 签名。请只从官方仓库下载，并将文件的 SHA-256 与发布页校验文件比对。[安装与校验帮助](${SUB1_REPO}/blob/main/docs/USAGE.zh-CN.md#安装)`,
      },
      {
        type: "faq",
        id: "faq",
        heading: "常见问题",
        items: [
          {
            q: "Meowcal Sub 能为没有字幕的视频生成字幕吗？",
            a: "不能。Meowcal Sub 用 OCR 读取画面文字，不转录语音。需要搜索字幕文件，或让字幕跟随播放进度时，可了解独立应用 [Meowcal Sub 2](@sub2)。",
          },
          {
            q: "可以离线使用吗？",
            a: "安装好本地引擎后，OCR 和翻译可以离线运行；使用 Windows OCR 时，还需要先安装识别语言。设置、修复下载、缺少的 Windows 语言组件以及更新检查需要联网。视频服务有自己的联网要求。",
          },
          {
            q: "是免费的吗？",
            a: "是。Meowcal Sub 以 AGPL-3.0-only 协议开源，无需账号、API 密钥或订阅，也没有按次翻译费用。",
          },
          {
            q: "可以翻译网页或幻灯片吗？",
            a: "设置中的 **翻译任意文字** 放宽字幕专用筛选，可翻译选区内的其他可读文字，默认关闭。效果仍取决于文字清晰度、语言支持和屏幕捕获是否可用。",
          },
          {
            q: "需要其他 Meow 应用吗？",
            a: "不需要。Meowcal Sub 使用随应用提供的 Meowcal Core 运行环境。Meowcal Sub 2 使用另一种字幕工作流，MeowWatch 用于一起观看，两者都不是运行前提。",
          },
        ],
      },
      {
        type: "products",
        id: "more",
        heading: "More Meow tools",
        intro: "面向观看的小工具，各自专注一个任务。",
        items: [
          {
            name: "Meowcal Sub 2",
            body: "搜索字幕源，并让字幕文件跟随视频播放进度。这是另一种工作流，不是必须升级到的版本。",
            href: "@sub2",
          },
          {
            name: "MeowWatch",
            body: "同步看视频，同时使用悬浮聊天。",
            href: "https://github.com/PeterShanxin/MeowWatch",
          },
        ],
      },
    ],
  },

  sub2: {
    title: "Meowcal Sub 2 – Windows 字幕搜索、同步与本地翻译",
    description:
      "Meowcal Sub 2 是 Windows 桌面字幕工作台：搜索 OpenSubtitles、SubDL 和 ASSRT，用屏幕 OCR 跟随播放进度，缺少的译文由本地翻译补上。免费开源。",
    name: "Meowcal Sub 2",
    navName: "Meowcal Sub 2",
    tagline: "字幕，用你想读的语言。",
    promise: "搜索 · 同步 · 本地翻译",
    lead: "Meowcal Sub 2 是一款 Windows 桌面字幕工作台。它读取屏幕上的字幕区域，对照字幕文件跟随播放进度，并在始终置顶的字幕条中显示目标语言。找不到匹配的译文时，可以在你的电脑上翻译。",
    actions: {
      latest: { label: "下载最新版本", hint: "安装版或便携版，x64 与 ARM64" },
      meta: "本页介绍版本 {version} · Windows x64 与 ARM64 · 以 AGPL-3.0-only 协议免费开源",
      links: [
        { label: "使用指南（英文）", href: `${SUB2_REPO}/blob/main/docs/user-guide.md` },
        { label: "源代码", href: SUB2_REPO },
      ],
    },
    figure: {
      image: "sub2-studio",
      alt: "Meowcal Sub 2 工作台：示例影片 Paper Lanterns 的会话已准备好，源字幕和目标字幕已选定，每一行都有译文，“Start OCR and sync” 按钮可用。",
      caption: "版本 {version} 的工作台，其中的片名为虚构示例。应用界面目前为英文。",
    },
    sections: [
      {
        type: "callout",
        title: "搜索字幕需要字幕源凭据。",
        body: "至少添加一项：OpenSubtitles API 密钥、SubDL API 密钥或 ASSRT 令牌。各字幕源的可用性和下载额度以其自身规定为准。",
      },
      {
        type: "pairs",
        id: "features",
        heading: "功能",
        termLabel: "任务",
        detailLabel: "Meowcal Sub 2 怎么做",
        items: [
          {
            term: "找字幕",
            detail: "在同一个工作台中搜索 OpenSubtitles、SubDL 和 ASSRT。",
          },
          {
            term: "跟随播放",
            detail: "用 OCR 匹配播放器中可见的字幕，确定当前播放位置。",
          },
          {
            term: "显示目标语言",
            detail:
              "按目标语言字幕文件自身的时间轴显示，包括重叠和静默间隔。文件没有覆盖的字幕，由已有译文或本地推理补上。",
          },
          {
            term: "播放前先准备",
            detail: "在播放前准备译文，整理字幕顺序并移除完全重复的条目。原始文件保持不变。",
          },
          {
            term: "修正字幕文件",
            detail:
              "同步前可编辑 SRT 和 WebVTT 的文本与时间，预览偏移校正，并在已准备的会话中直接使用修正后的副本。",
          },
          {
            term: "边看边调整",
            detail: "播放过程中可调整字幕时间。捕获区域和字幕外观都在应用内设置。",
          },
        ],
      },
      {
        type: "steps",
        id: "how-it-works",
        heading: "使用方法",
        items: [
          {
            title: "配置",
            body: "打开 **Settings**，启用至少一个字幕源，并选择语言组合。",
          },
          {
            title: "搜索并选择",
            body: "搜索片名。先选择源字幕，再选择目标字幕或本地翻译。",
          },
          {
            title: "同步并观看",
            body: "圈住播放器中的字幕，然后开始会话。工作台会变成带控制栏的字幕条。结束后在控制栏停止会话。",
          },
        ],
      },
      {
        type: "columns",
        id: "requirements",
        heading: "系统要求与边界",
        columns: [
          {
            title: "你需要准备",
            items: [
              "Windows，x64 或 ARM64，并安装 Microsoft WebView2。",
              "与画面文字语言对应的 Windows OCR 语言包。",
              "至少一项字幕源凭据，用于搜索。",
              "使用本地翻译时：一次性下载约 1.1 GB 的模型，另加运行环境。已准备好的目标字幕无需该模型即可使用，但未翻译的空缺会保持空白。",
            ],
          },
          {
            title: "也有边界",
            items: [
              "OCR 与匹配效果取决于视频、字幕版本、语言和圈选区域。",
              "这是早期版本，不保证每个播放器或每部影片都能匹配。",
              "受保护的视频画面可能无法捕获。",
            ],
          },
        ],
        body: "使用 Meowcal Sub 2 不需要安装 Meowcal Sub。",
      },
      {
        type: "columns",
        id: "privacy",
        heading: "数据与联网",
        columns: [
          {
            title: "留在你的电脑上",
            items: ["文字识别（OCR）", "翻译推理", "配置、字幕文件和日志，保存在用户配置目录"],
          },
          {
            title: "需要联网的情况",
            items: [
              "向你启用的字幕源搜索并下载字幕",
              "可选的 TMDb 元数据和 Google Fonts",
              "安装引擎与模型",
              "安装版应用的更新检查与下载",
            ],
          },
        ],
        body: `日志中可能出现片名、错误信息和其他敏感内容，分享前请先脱敏。详见[安全策略](${SUB2_REPO}/blob/main/SECURITY.md)。`,
      },
      {
        type: "products",
        id: "more",
        heading: "该选哪款 Meowcal 应用？",
        intro: "Meowcal Sub 2 是另一种工作流，不是必须升级到的版本。",
        items: [
          {
            name: "Meowcal Sub · 妙影",
            body: "直接识别屏幕上的字幕并在本地翻译。正常使用不需要字幕文件或字幕源密钥。",
            href: "@sub1",
          },
          {
            name: "Meowcal Sub 2",
            body: "搜索字幕源，对照字幕文件跟随播放进度，并显示目标字幕文件中的字幕。缺少的部分由本地翻译补上。",
            current: "当前页面",
          },
          {
            name: "MeowWatch",
            body: "同步看视频，同时使用悬浮聊天。",
            href: "https://github.com/PeterShanxin/MeowWatch",
          },
        ],
      },
    ],
  },
};
