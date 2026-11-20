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
            term: "已经找到一份喜欢的字幕译文",
            detail:
              "用 [Meowcal Sub 2](@compare) 把现成字幕自动对齐到视频。保留字幕创作者打磨好的译文，让台词跟上播放进度。",
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
              "可以短暂暂停，留出阅读时间。如果已有喜欢的字幕译文，也可以用 [Sub 2](@compare) 直接显示现成字幕，自动对齐视频。",
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
            body: "日常观看的省心之选，或让现成好字幕自动跟上视频。",
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
      "Meowcal Sub · 妙影是省心百搭的日常字幕翻译工具。电影、剧集已有喜欢的字幕译文时，用 Sub 2 自动对齐播放进度。看看哪款适合你。",
    name: "Meowcal Sub 和 Sub 2，怎么选？",
    lead: "Meowcal Sub · 妙影是日常看视频的省心之选：圈出画面里的字幕，直接看译文，简单百搭。电影、剧集已经有字幕组等创作者做好的翻译时，Sub 2 帮你自动对齐播放进度，让你看上精心制作的字幕。",
    sections: [
      {
        type: "pairs",
        id: "choose",
        heading: "按你想做的事来选",
        termLabel: "你想做什么",
        detailLabel: "推荐选择",
        items: [
          {
            term: "想省心看懂各种视频里的外语字幕",
            detail:
              "选 [Meowcal Sub · 妙影](@sub1)。日常观看的百搭选择，圈出画面文字就能翻译，无需字幕文件、账号或 API key。",
          },
          {
            term: "电影、剧集已有别人做好的优质字幕",
            detail:
              "选 [Meowcal Sub 2](@sub2)。找到喜欢的字幕版本，让应用自动对齐播放进度，读字幕作者打磨好的译文。热门或字幕资源齐全的影视作品尤其适合。",
          },
          {
            term: "喜欢的字幕和视频对不上时间",
            detail:
              "选 [Meowcal Sub 2](@sub2)。它根据画面里的原字幕找到当前播放位置，把你选好的译文对齐到对应的台词。",
          },
          {
            term: "和另一个人同步看视频",
            detail:
              "[MeowWatch](https://github.com/PeterShanxin/MeowWatch) 可以同步播放，并提供悬浮聊天。",
          },
        ],
      },
      {
        type: "faq",
        id: "questions",
        heading: "几个常见选择",
        items: [
          {
            q: "用了 Sub1，还要升级到 Sub2 吗？",
            a: "两款应用都可以独立使用。日常随看随译用 Sub1，想欣赏现成字幕的译文时用 Sub2。",
          },
          {
            q: "第一次用，推荐哪个？",
            a: "拿不准选哪个，就从 [妙影](@hardcoded) 开始：圈出画面字幕，直接看译文。电影、剧集已经有你喜欢的字幕翻译时，再用 Sub2 让它跟上视频。",
          },
          {
            q: "视频画面没有字幕，Sub2 还能自动同步吗？",
            a: "妙影2 需要读到画面上的字幕，才能找到播放位置。请先在播放器中打开字幕。即使只有译文文件，它也可以先翻译画面上的几句对白，再去文件里找对应的场景。",
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
          { name: "Meowcal Sub 2", body: "用现成的好字幕，自动对齐播放进度。", href: "@sub2" },
        ],
      },
    ],
  },
};
