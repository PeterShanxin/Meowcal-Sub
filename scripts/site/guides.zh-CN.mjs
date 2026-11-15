const REPO = "https://github.com/PeterShanxin/Meowcal-Sub";

export default {
  hardcoded: {
    title: "视频只有外语字幕，怎么看中文？Windows 屏幕字幕翻译教程 | 妙影",
    description:
      "找不到中文字幕文件？用 Meowcal Sub · 妙影圈出视频自带的外语字幕，在电脑上识别、翻译并悬浮显示中文。无需上传视频、注册账号或填写 API key。",
    name: "视频只有外语字幕，怎么看中文？",
    lead: "视频有英文、日文等外语字幕，却找不到中文字幕？在 Windows 11 上打开 Meowcal Sub · 妙影，圈出画面中的字幕，就能在悬浮窗口里看译文。文字识别和翻译都在本机完成，无需另找字幕文件、注册账号或填写 API key。",
    sections: [
      {
        type: "pairs",
        id: "subtitle-source",
        heading: "先看看视频里有什么字幕",
        termLabel: "你的情况",
        detailLabel: "怎么观看",
        items: [
          {
            term: "字幕直接在视频画面里，关不掉",
            detail:
              "这类内嵌在画面中的字幕可以直接识别。妙影读取屏幕上的文字，不需要提取字幕文件。",
          },
          {
            term: "播放器可以切换字幕轨道",
            detail: "打开已有的外语字幕，圈住字幕所在的位置，就可以翻译。",
          },
          {
            term: "已经找到与视频匹配的字幕文件",
            detail:
              "可以用 [Meowcal Sub 2](@compare) 对照画面中的文字跟随播放进度，显示目标语言的字幕，缺少的译文由本地翻译补上。",
          },
          {
            term: "画面上完全没有字幕",
            detail:
              "先在播放器中开启已有字幕。如果没有字幕轨道，需要先用语音转文字工具生成字幕，再进行屏幕翻译。",
          },
        ],
      },
      {
        type: "steps",
        id: "setup",
        heading: "三步看上中文翻译",
        items: [
          {
            title: "安装并选择语言",
            body: "[下载妙影](@sub1#download)，按设置向导下载并测试本地翻译引擎。在首页将 **原字幕** 设为视频中的语言，**翻译为** 选为简体中文，也可以选择其他支持的语言。",
          },
          {
            title: "圈出字幕区域",
            body: "播放到有字幕的画面，把妙影主窗口移到播放视频的屏幕，点击 **选择字幕区域**。圈住字幕所在的一条区域，留出双行字幕的位置，再点击 **确认并开始**。",
          },
          {
            title: "边看边读译文",
            body: "观看时保持原字幕显示。在 **字幕样式** 中调整文字大小和深色／浅色背景。看完后点击 **停止翻译**。",
          },
        ],
      },
      {
        type: "pairs",
        id: "adjustments",
        heading: "遇到这些情况，怎么调整",
        termLabel: "情况",
        detailLabel: "处理方法",
        items: [
          {
            term: "移动了视频窗口，或者切换了全屏",
            detail: "重新选择字幕区域。保存的选区对应屏幕上的固定位置。",
          },
          {
            term: "字幕漏字或识别不准",
            detail:
              "检查选区有没有圈全、原字幕语言是否选对。字太小时，可以放大视频窗口。在 **设置** 中可切换 PP-OCRv6 和 Windows OCR；韩语使用 Windows OCR，需要安装韩语识别组件。",
          },
          {
            term: "译文跟不上播放",
            detail:
              "可以短暂暂停，留出阅读时间。如果找得到匹配的字幕文件，也可以用 [Sub 2](@compare) 在播放前准备译文。",
          },
          {
            term: "捕获到的视频画面是空白的",
            detail:
              "受保护的视频可能禁止屏幕捕获。使用视频平台自带的字幕选项，或换用允许捕获的视频来源。",
          },
        ],
      },
      {
        type: "faq",
        id: "questions",
        heading: "开始前你可能想知道",
        items: [
          {
            q: "会把中文字幕加进视频文件吗？",
            a: "译文显示在悬浮窗口里，供你观看时阅读。原视频保持不变，这个流程不导出带中文字幕的视频。",
          },
          {
            q: "断网也能用吗？",
            a: "完成首次设置并装好所需识别语言后，文字识别和翻译可以离线运行。想断网观看，视频也需要提前保存在电脑上。详见[下载、联网和本地日志说明](@sub1#privacy)。",
          },
        ],
      },
      {
        type: "products",
        id: "next",
        heading: "开始使用",
        intro: "按你手上的视频选择合适的方式。",
        items: [
          {
            name: "下载妙影",
            body: "适用于 Windows 11，提供 x64 和 ARM64 安装包。",
            href: "@sub1#download",
          },
          {
            name: "Sub1、Sub2 怎么选",
            body: "直接翻译画面字幕，还是提前准备字幕文件。",
            href: "@compare",
          },
          {
            name: "设置与故障排查",
            body: "查看设置项、识别语言，以及常见问题的处理方法。",
            href: `${REPO}/blob/main/docs/USAGE.zh-CN.md`,
          },
        ],
      },
    ],
  },
  compare: {
    title: "Meowcal Sub 和 Sub 2 有什么区别？字幕翻译工具怎么选",
    description:
      "直接翻译视频画面里的字幕，选 Meowcal Sub · 妙影；想搜索字幕文件、提前准备译文并跟随播放进度，选 Sub 2。比较两款 Windows 字幕工具的用途和准备步骤。",
    name: "Meowcal Sub 和 Sub 2，怎么选？",
    lead: "想直接翻译画面里的字幕，选 Meowcal Sub · 妙影。想找字幕文件、提前准备译文，并让字幕跟随播放进度，选 Meowcal Sub 2。两款应用都在 Windows 上独立运行，文字识别和翻译在本机完成。",
    sections: [
      {
        type: "pairs",
        id: "choose",
        heading: "按你想做的事来选",
        termLabel: "你想做什么",
        detailLabel: "推荐选择",
        items: [
          {
            term: "看懂画面上的外语字幕，不想找字幕文件",
            detail:
              "选 [Meowcal Sub · 妙影](@sub1)。圈出字幕，直接看悬浮译文，无需字幕源账号或 API key。",
          },
          {
            term: "为电影或剧集找一份字幕",
            detail:
              "选 [Meowcal Sub 2](@sub2)。在 Settings 中配置密钥或令牌后，搜索 OpenSubtitles、SubDL 或 ASSRT。",
          },
          {
            term: "使用现成译文，让字幕跟上视频",
            detail:
              "选 [Meowcal Sub 2](@sub2)。它把画面中的字幕与原文字幕文件匹配，确定播放位置，再显示目标字幕。也可以调整时间、在观看前准备译文。",
          },
          {
            term: "和另一个人同步看视频",
            detail:
              "[MeowWatch](https://github.com/PeterShanxin/MeowWatch) 可以同步播放，并提供悬浮聊天。",
          },
        ],
      },
      {
        type: "columns",
        id: "prepare",
        heading: "各自需要准备什么",
        columns: [
          {
            title: "Meowcal Sub · 妙影",
            items: [
              "Windows 11，x64 或 ARM64。",
              "画面中有可见字幕，字幕区域可以被应用捕获。",
              "按向导安装本地翻译引擎。识别韩语或使用 Windows OCR 时，还需安装原字幕语言的 Windows 识别组件。",
            ],
          },
          {
            title: "Meowcal Sub 2",
            items: [
              "Windows，x64 或 ARM64，并安装 Microsoft WebView2。",
              "用于搜索的字幕源密钥或令牌，以及与画面文字匹配的原文字幕文件。",
              "画面文字对应的 Windows OCR 语言包。需要提前翻译或补译缺失内容时，再安装本地翻译模型。",
            ],
          },
        ],
        body: "Sub 2 可以直接使用已经准备好的目标语言字幕，无需翻译模型；没有译文的空缺会保持空白。",
      },
      {
        type: "faq",
        id: "questions",
        heading: "几个常见选择",
        items: [
          {
            q: "用了 Sub1，还要升级到 Sub2 吗？",
            a: "按用途选一款就可以。Sub2 独立运行，两款应用都随附 Meowcal Core 运行环境。",
          },
          {
            q: "第一次用，推荐哪个？",
            a: "视频有外语字幕，但找不到合适的中文译文，先试 [妙影](@hardcoded)。想用现成字幕，或在观看前准备好译文，先试 Sub2。",
          },
          {
            q: "视频画面没有字幕，Sub2 还能自动同步吗？",
            a: "自动匹配播放位置需要画面中的文字与原文字幕文件对应。开始 OCR 同步前，请在播放器中开启原字幕。",
          },
          {
            q: "哪些步骤需要联网？",
            a: "使用本地翻译时，两款应用都需要先下载翻译引擎。Sub1 随后在本机翻译；Sub2 搜索和下载字幕时，还会连接你启用的字幕源。详见 [Sub1 联网说明](@sub1#privacy)和 [Sub2 联网说明](@sub2#privacy)。",
          },
        ],
      },
      {
        type: "products",
        id: "next",
        heading: "下载应用",
        intro: "产品页提供下载入口和设置步骤。",
        items: [
          { name: "Meowcal Sub · 妙影", body: "直接翻译屏幕上已有的字幕。", href: "@sub1" },
          { name: "Meowcal Sub 2", body: "搜索、准备并同步字幕文件。", href: "@sub2" },
        ],
      },
    ],
  },
};
