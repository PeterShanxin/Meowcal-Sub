const REPO = "https://github.com/PeterShanxin/Meowcal-Sub";

export default {
  hardcoded: {
    title: "Translate hardcoded subtitles while watching on Windows | Meowcal Sub",
    description:
      "Translate burned-in or on-screen subtitles without an SRT file. Select the subtitle area in Meowcal Sub and read a local AI translation in a floating overlay.",
    name: "How to translate hardcoded subtitles while watching",
    lead: "When a video has subtitles in a language you cannot read, Meowcal Sub can translate the text visible on screen. Select the subtitle area, choose your language, and read the translation in a floating overlay on Windows 11. You do not need an SRT file, an account, or an API key.",
    sections: [
      {
        type: "pairs",
        id: "subtitle-source",
        heading: "Start with the subtitles you have",
        termLabel: "Your video",
        detailLabel: "How to watch",
        items: [
          {
            term: "Subtitles are part of the picture",
            detail:
              "Hardcoded or burned-in subtitles stay in the picture even when a player's subtitle track is turned off. Meowcal Sub reads those words directly from the screen.",
          },
          {
            term: "The player displays a subtitle track",
            detail:
              "Keep that track visible and select its text. Screen translation works from the displayed words, regardless of where the track came from.",
          },
          {
            term: "You have a matching subtitle file",
            detail:
              "[Meowcal Sub 2](@compare) can use subtitle files and match visible text to the playback position. It can show a target-language track and translate missing lines locally.",
          },
          {
            term: "There are no words on screen",
            detail:
              "Enable an available subtitle track first. If there is none, you need a speech-to-text tool to create captions before screen translation can read them.",
          },
        ],
      },
      {
        type: "steps",
        id: "setup",
        heading: "Translate the subtitles on your screen",
        items: [
          {
            title: "Install and choose your languages",
            body: "[Download Meowcal Sub](@sub1#download) for your Windows 11 PC. Complete the setup wizard, which downloads and tests the local translation engine. In Home, set **Original subtitles** to the language in the video and **Translate into** to the language you want to read.",
          },
          {
            title: "Frame the subtitle area",
            body: "Play a scene with a visible subtitle. Move the Meowcal Sub window to the video's screen and choose **Select subtitle area**. Draw a box around the subtitle band, leaving room for two-line captions, then choose **Confirm and start**.",
          },
          {
            title: "Read the floating translation",
            body: "Keep the original subtitles visible while watching. In **Subtitle style**, adjust text size and choose a Dark or Light background. When finished, choose **Stop translation**.",
          },
        ],
      },
      {
        type: "pairs",
        id: "adjustments",
        heading: "Keep the text easy to read",
        termLabel: "What changed",
        detailLabel: "What to do",
        items: [
          {
            term: "You moved the video or switched to fullscreen",
            detail:
              "Select the subtitle area again. The saved box refers to a fixed area of the screen.",
          },
          {
            term: "Words are missing or misread",
            detail:
              "Check that the box includes the full subtitle and that the original language is correct. Try a larger video window for small text. In **Settings**, you can switch between PP-OCRv6 and Windows OCR; Korean uses Windows OCR and needs its recognition-language component.",
          },
          {
            term: "The translation falls behind",
            detail:
              "A short pause gives you time to read. For a video with matching subtitle files, consider [Sub 2](@compare), which can prepare translations before playback.",
          },
          {
            term: "Capture shows a blank video",
            detail:
              "Protected video can block screen capture. Use the video's own subtitle options or another source that permits capture.",
          },
        ],
      },
      {
        type: "faq",
        id: "questions",
        heading: "Before you start",
        items: [
          {
            q: "Does it change the video file?",
            a: "The translation appears in a floating window while you watch. Your video stays unchanged; this workflow does not export a subtitled video.",
          },
          {
            q: "Can I use it offline?",
            a: "Once setup and any required recognition-language installation are complete, OCR and translation run locally. For offline viewing, keep the video on your PC too. See [downloads, network use, and local logs](@sub1#privacy).",
          },
        ],
      },
      {
        type: "products",
        id: "next",
        heading: "Start watching",
        intro: "Choose the workflow that fits your video.",
        items: [
          {
            name: "Download Meowcal Sub",
            body: "Windows 11, with x64 and ARM64 installers.",
            href: "@sub1#download",
          },
          {
            name: "Compare Sub and Sub 2",
            body: "Screen translation or a prepared subtitle track.",
            href: "@compare",
          },
          {
            name: "Setup and troubleshooting",
            body: "Settings, recognition languages, and help with common problems.",
            href: `${REPO}/blob/main/docs/USAGE.md`,
          },
        ],
      },
    ],
  },
  compare: {
    title: "Meowcal Sub vs Sub 2: which subtitle tool should you use?",
    description:
      "Choose Meowcal Sub for translating visible subtitles without subtitle files. Choose Sub 2 for subtitle search, playback matching, and prepared translations on Windows.",
    name: "Meowcal Sub or Sub 2: which should you use?",
    lead: "Choose Meowcal Sub to translate the subtitles already on screen. Choose Meowcal Sub 2 to find subtitle files, match them to playback, and use a prepared translation. Both are independent Windows apps with local OCR and translation.",
    sections: [
      {
        type: "pairs",
        id: "choose",
        heading: "Choose by what you want to do",
        termLabel: "Your goal",
        detailLabel: "Start here",
        items: [
          {
            term: "Translate visible subtitles without hunting for a file",
            detail:
              "[Meowcal Sub](@sub1). Select the text on screen and read the floating translation. No subtitle-provider account or API key is needed.",
          },
          {
            term: "Find a subtitle track for a film or episode",
            detail:
              "[Meowcal Sub 2](@sub2). Search OpenSubtitles, SubDL, or ASSRT after adding a provider key or token in Settings.",
          },
          {
            term: "Use a translated track and keep it aligned",
            detail:
              "[Meowcal Sub 2](@sub2). Match visible subtitles against the source track to follow playback, then display the target track. You can adjust timing and prepare translations before watching.",
          },
          {
            term: "Watch together on two computers",
            detail:
              "[MeowWatch](https://github.com/PeterShanxin/MeowWatch) synchronizes playback and adds floating chat.",
          },
        ],
      },
      {
        type: "columns",
        id: "prepare",
        heading: "What to prepare",
        columns: [
          {
            title: "Meowcal Sub",
            items: [
              "Windows 11 on x64 or ARM64.",
              "A video with visible subtitles and an area the app can capture.",
              "Complete the local translation engine setup. Korean or Windows OCR also needs the original language's Windows OCR component.",
            ],
          },
          {
            title: "Meowcal Sub 2",
            items: [
              "Windows on x64 or ARM64, with Microsoft WebView2.",
              "A subtitle-provider credential for search, plus source subtitles that match the words visible in the video.",
              "The Windows OCR language pack for the text on screen. Install the local translation model if you want to prepare translations or fill missing lines.",
            ],
          },
        ],
        body: "A prepared target-language track in Sub 2 can be used without the translation model; any untranslated gaps remain empty.",
      },
      {
        type: "faq",
        id: "questions",
        heading: "Common choices",
        items: [
          {
            q: "Is Sub 2 an upgrade I need to install?",
            a: "Pick either app for its workflow. Sub 2 runs independently, and both apps include their Meowcal Core runtime.",
          },
          {
            q: "Which one should I try first?",
            a: "For a video with foreign-language subtitles and no suitable translated track, start with [Meowcal Sub](@hardcoded). If you want to use an existing translation or prepare one before watching, start with Sub 2.",
          },
          {
            q: "Can Sub 2 follow a video with no visible subtitles?",
            a: "Automatic playback matching needs visible text that corresponds to the source subtitle file. Enable the original subtitles in your player before starting OCR sync.",
          },
          {
            q: "What needs the internet?",
            a: "For local translation, both apps download a translation engine during setup. Sub 1 then translates on your PC. Sub 2 also connects to the subtitle providers you enable for search and downloads. See [Sub 1 network use](@sub1#privacy) and [Sub 2 network use](@sub2#privacy).",
          },
        ],
      },
      {
        type: "products",
        id: "next",
        heading: "Get the app",
        intro: "Each product page has its downloads and setup instructions.",
        items: [
          {
            name: "Meowcal Sub",
            body: "Translate the subtitles already on your screen.",
            href: "@sub1",
          },
          {
            name: "Meowcal Sub 2",
            body: "Find, prepare, and sync subtitle tracks.",
            href: "@sub2",
          },
        ],
      },
    ],
  },
};
