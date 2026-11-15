// English copy for the product site. content.zh-CN.mjs mirrors this structure.
//
// Text supports **bold** and [label](target). A target starting with "@" names
// another page of this site in the same language: @sub1 or @sub2.

const SUB1_REPO = "https://github.com/PeterShanxin/Meowcal-Sub";
const SUB2_REPO = "https://github.com/PeterShanxin/Meowcal-Sub-2";

export default {
  lang: "en",
  ogLocale: "en_US",
  languageName: "English",
  skipLink: "Skip to content",
  navLabel: "Products",
  languageLabel: "Language",
  source: "GitHub",
  downloadLabel: "Download for Windows",
  contentsLabel: "On this page",
  howLabel: "See how it works",
  footer: {
    links: [
      { label: "Meowcal Sub source", href: SUB1_REPO },
      { label: "Meowcal Sub 2 source", href: SUB2_REPO },
      { label: "Name and logo use", href: `${SUB1_REPO}/blob/main/TRADEMARKS.md` },
      { label: "Report a problem", href: "@issues" },
    ],
  },

  sub1: {
    title: "Meowcal Sub – Translate on-screen subtitles locally on Windows 11",
    description:
      "Meowcal Sub reads the subtitles already on your screen and translates them on your PC. Free, open-source OCR and AI translation for Windows 11 on x64 and ARM64.",
    name: "Meowcal Sub",
    navName: "Meowcal Sub",
    tagline: "Your go-to for on-screen subtitle translation.",
    promise: "Local translation. No account. No API key.",
    lead: "Meowcal Sub is a versatile choice for everyday watching on Windows. Draw a box around the subtitles and read the translation in a floating overlay. It works with visible or burned-in text, translates on your PC, and needs no separate subtitle file, account, or API key.",
    actions: {
      x64: { label: "Download for x64", hint: "Intel or AMD PC" },
      arm64: { label: "Download for ARM64", hint: "Snapdragon or other Windows on ARM PC" },
      store: { label: "Get it from Microsoft Store" },
      meta: "Version {version} · Windows 11 public beta · Free and open source under AGPL-3.0-only",
      links: [
        {
          label: "Translation model terms",
          href: `${SUB1_REPO}/blob/main/LICENSE-NOTICE.md#separately-downloaded-translation-model`,
        },
        { label: "Release notes and all files", href: "{notes}" },
        { label: "User guide", href: `${SUB1_REPO}/blob/main/docs/USAGE.md` },
        { label: "Source code", href: SUB1_REPO },
      ],
    },
    figure: {
      image: "sub1-overlay",
      alt: "A film scene with the Chinese subtitle “海面很平静，我们还有时间。” inside the selected area. The floating overlay below it shows the English translation “The sea is quiet. We still have time.”",
      caption: "Screenshots show the app interface with an authored sample subtitle.",
    },
    sections: [
      {
        type: "steps",
        id: "how-it-works",
        heading: "How it works",
        items: [
          {
            title: "Set up translation",
            body: "Open the app and complete the guided setup. Setup downloads and tests the local translation engine. Then choose the original subtitle language and the language you want to read.",
            image: "sub1-home",
            alt: "The Meowcal Sub Home window with the Original subtitles and Translate into lists, and the Select subtitle area and Start with saved area buttons.",
          },
          {
            title: "Select the subtitle area",
            body: "Move the app window onto the screen that plays the video and choose **Select subtitle area**. Draw a box around the original subtitles, not the whole video. Then choose **Confirm and start**.",
            image: "sub1-selection",
            alt: "The subtitle-area selector with a box drawn around a subtitle, and the Confirm and start, Redraw, Save and exit, and Exit buttons.",
          },
          {
            title: "Watch",
            body: "Read the translation in the floating overlay. **Subtitle style** sets the text size and a Dark or Light background. Choose **Exit translation** or **Stop translation** when you are done.",
            image: "sub1-style",
            alt: "The Subtitle style page with a live preview, a Text size slider, and a Dark or Light background choice.",
          },
        ],
      },
      {
        type: "pairs",
        id: "features",
        heading: "Made for watching",
        termLabel: "What you need",
        detailLabel: "What Meowcal Sub does",
        items: [
          {
            term: "Translate visible subtitles without finding a separate subtitle file",
            detail:
              "Reads the selected screen region with the built-in PP-OCRv6 recognizer, or Windows OCR if you choose it.",
          },
          {
            term: "Keep captured and translated words off cloud services",
            detail: "Runs OCR and Tencent HY-MT translation on your PC.",
          },
          {
            term: "Read the translation without leaving the video",
            detail:
              "Shows an always-on-top overlay with adjustable text size and a Dark or Light background.",
          },
          {
            term: "Translate between the languages you watch in",
            detail:
              "Choose the original and translation languages from Chinese (Simplified), Chinese (Traditional), Japanese, Korean, English, Spanish, French, and German.",
          },
          {
            term: "Translate readable text beyond subtitles",
            detail:
              "**Translate any text** in Settings relaxes the subtitle-only filter. It is off by default.",
          },
          {
            term: "Keep the local engine working",
            detail:
              "Guided setup, integrity checks, engine repair, and signature-verified in-app updates.",
          },
        ],
      },
      {
        type: "columns",
        id: "privacy",
        heading: "Offline translation and privacy",
        columns: [
          {
            title: "Stays on your PC",
            items: [
              "Capture of the selected region",
              "Text recognition (OCR)",
              "Translation inference",
              "The floating overlay",
            ],
          },
          {
            title: "Uses the network",
            items: [
              "Engine and model downloads for setup or repair",
              "Windows recognition-language installation when needed",
              "Application update checks and downloads",
            ],
          },
        ],
        body: "After setup, OCR and translation run on your PC without uploading the video or subtitle text. For offline viewing, download the video too. Default logs contain timings and support codes; the optional Verbose setting also saves captures and subtitle text locally.",
      },
      {
        type: "columns",
        id: "requirements",
        heading: "Requirements and limits",
        columns: [
          {
            title: "What you need",
            items: [
              "Windows 11 on x64 or ARM64.",
              "At least 8 GiB of system RAM reported by Windows, and 3 GiB free on the engine installation drive. Translation speed depends on your PC and the text.",
              "An internet connection for first-time setup, which downloads the local translation runtime and model, about 1.1 GB.",
              "For Korean subtitles, or when you choose Windows OCR: the Windows recognition language for the original subtitles. The built-in PP-OCRv6 recognizer needs no Windows language.",
            ],
          },
          {
            title: "During playback",
            items: [
              "Each session translates one screen: the screen that contains the app when you select the subtitle area.",
              "The subtitle area is a fixed screen region. Select it again when the video moves or changes size.",
              "Recognition depends on the source language, text clarity, and what screen capture can see. Stylized fonts, fast-changing text, and protected video can cause missing or incorrect results.",
            ],
          },
        ],
        body: `**Windows may show an unknown-publisher warning.** The installers on GitHub are not Authenticode-signed. Download them only from the official repository and compare the file's SHA-256 with the release checksums. [Installation and checksum help](${SUB1_REPO}/blob/main/docs/USAGE.md#installation)`,
      },
      {
        type: "faq",
        id: "faq",
        heading: "Frequently asked questions",
        items: [
          {
            q: "Can Meowcal Sub create subtitles for a video that has none?",
            a: "Meowcal Sub translates visible text. For a video without captions, use a speech-to-text tool first. [Choose a subtitle workflow](@compare).",
          },
          {
            q: "Does it work offline?",
            a: "Yes. Complete engine setup and install any needed Windows OCR language component first. OCR and translation then run locally; online video playback and application updates still use the network.",
          },
          {
            q: "Is it free?",
            a: "Yes. Meowcal Sub is open source under AGPL-3.0-only. It needs no account, API key, subscription, or per-translation fee.",
          },
          {
            q: "Can it translate web pages or slides?",
            a: "**Translate any text** in Settings relaxes the subtitle-specific filter for readable text in the selected region. It is off by default. Recognition still depends on text clarity, language support, and screen-capture availability.",
          },
          {
            q: "Do I need another Meow app?",
            a: "No. Meowcal Sub uses its packaged Meowcal Core runtime. Meowcal Sub 2 has a separate subtitle workflow, and MeowWatch is for watching together. Neither is required.",
          },
        ],
      },
      {
        type: "products",
        id: "guides",
        heading: "Find the right subtitle workflow",
        intro: "Start with the subtitles your video already has.",
        items: [
          {
            name: "Translate hardcoded subtitles",
            body: "Read burned-in or on-screen subtitles in your language, without an SRT file.",
            href: "@hardcoded",
          },
          {
            name: "Meowcal Sub or Sub 2?",
            body: "An easy everyday translator, or existing subtitles synced to your video.",
            href: "@compare",
          },
        ],
      },
      {
        type: "products",
        id: "more",
        heading: "More Meow tools",
        intro: "Small tools for watching, each with a focused task.",
        items: [
          {
            name: "Meowcal Sub 2",
            body: "Use existing translations made for your film or series, automatically aligned to playback.",
            href: "@sub2",
          },
          {
            name: "MeowWatch",
            body: "Watch together with synchronized playback and floating chat.",
            href: "https://github.com/PeterShanxin/MeowWatch",
          },
        ],
      },
    ],
  },

  sub2: {
    title: "Meowcal Sub 2 – Automatically sync existing subtitles on Windows",
    description:
      "Watch with existing subtitles on Windows. Meowcal Sub 2 finds subtitle files and automatically aligns them to playback, so you can enjoy a translation made for your film or series.",
    name: "Meowcal Sub 2",
    navName: "Meowcal Sub 2",
    tagline: "Great subtitles. In sync with your video.",
    promise: "Find subtitles. Let them follow the film.",
    lead: "Found a good subtitle translation for your film or series? Meowcal Sub 2 automatically lines it up with what you are watching. Enjoy the wording, tone, and context a subtitle creator has worked on, with the right line appearing at the right moment.",
    actions: {
      latest: {
        label: "Download the latest release",
        hint: "Installer or portable, x64 and ARM64",
      },
      meta: "This page describes version {version} · Windows x64 and ARM64 · Free and open source under AGPL-3.0-only",
      links: [
        { label: "User guide", href: `${SUB2_REPO}/blob/main/docs/user-guide.md` },
        { label: "Source code", href: SUB2_REPO },
      ],
    },
    figure: {
      image: "sub2-studio",
      alt: "The Meowcal Sub 2 studio with a prepared session for the sample title Paper Lanterns. The source and target subtitle files are chosen, every line is answered, and the Start OCR and sync button is ready.",
      caption: "The studio in version {version}, with invented sample titles.",
    },
    sections: [
      {
        type: "callout",
        title: "Subtitle search needs a provider credential.",
        body: "Add at least one: an OpenSubtitles API key, a SubDL API key, or an ASSRT token. Provider availability and download quotas apply.",
      },
      {
        type: "pairs",
        id: "features",
        heading: "What it does",
        termLabel: "Task",
        detailLabel: "What Meowcal Sub 2 does",
        items: [
          {
            term: "Find a translation made for your film",
            detail:
              "Search OpenSubtitles, SubDL, and ASSRT for existing subtitles in your language.",
          },
          {
            term: "Automatically align subtitles to playback",
            detail:
              "Recognizes where you are in the film from its visible subtitles and lines up your chosen track.",
          },
          {
            term: "Keep the translation you chose",
            detail: "Shows the subtitle translation you chose in a floating strip as you watch.",
          },
          {
            term: "Prepare before you watch",
            detail:
              "Prepares translations before playback, orders subtitle cues, and removes exact duplicates. Original files stay intact.",
          },
          {
            term: "Fix a subtitle file",
            detail:
              "Edits SRT and WebVTT text and timing before sync, previews drift corrections, and uses the corrected copies in the prepared session.",
          },
          {
            term: "Adjust while watching",
            detail:
              "Adjusts subtitle timing during playback. The capture region and subtitle appearance are set in the app.",
          },
        ],
      },
      {
        type: "steps",
        id: "how-it-works",
        heading: "How it works",
        items: [
          {
            title: "Configure",
            body: "Open **Settings**. Turn on at least one subtitle source and choose the language pair.",
          },
          {
            title: "Search and choose",
            body: "Search for the title. Choose a source subtitle, then a target subtitle or local translation.",
          },
          {
            title: "Sync and watch",
            body: "Drag a box around the player's subtitles and start the session. The studio becomes the subtitle strip with a live dock. Stop the session from the dock when you are done.",
          },
        ],
      },
      {
        type: "columns",
        id: "requirements",
        heading: "Requirements and limits",
        columns: [
          {
            title: "What you need",
            items: [
              "Windows on x64 or ARM64, with Microsoft WebView2.",
              "The Windows OCR language pack for the language on screen.",
              "At least one subtitle provider credential for search.",
              "For local translation: a one-time download of about 1.1 GB, plus the runtime. A prepared target subtitle works without it, but untranslated gaps then stay empty.",
            ],
          },
          {
            title: "During playback",
            items: [
              "OCR and matching depend on the video, the subtitle release, the language, and the selected region.",
              "Protected video surfaces can prevent screen capture.",
            ],
          },
        ],
        body: "Meowcal Sub does not need to be installed to use Meowcal Sub 2.",
      },
      {
        type: "columns",
        id: "privacy",
        heading: "Data and network use",
        columns: [
          {
            title: "Stays on your PC",
            items: [
              "Text recognition (OCR)",
              "Translation inference",
              "Configuration, subtitle files, and logs, in your user profile",
            ],
          },
          {
            title: "Uses the network",
            items: [
              "Subtitle search and download from the providers you enable",
              "Optional TMDb metadata and Google Fonts",
              "Engine and model installation",
              "Application update checks and downloads for installed copies",
            ],
          },
        ],
        body: `Titles, errors, and other sensitive details can appear in logs. Redact logs before you share them. See the [security policy](${SUB2_REPO}/blob/main/SECURITY.md).`,
      },
      {
        type: "products",
        id: "more",
        heading: "Which Meowcal app?",
        intro: "[Compare Sub and Sub 2](@compare) to choose a subtitle workflow.",
        items: [
          {
            name: "Meowcal Sub",
            body: "Your versatile everyday subtitle translator. Select the words on screen and read the translation, without finding a subtitle file.",
            href: "@sub1",
          },
          {
            name: "Meowcal Sub 2",
            body: "Use a translation made for your film or series and automatically align it to playback.",
            current: "This page",
          },
          {
            name: "MeowWatch",
            body: "Watch together with synchronized playback and floating chat.",
            href: "https://github.com/PeterShanxin/MeowWatch",
          },
        ],
      },
    ],
  },
};
