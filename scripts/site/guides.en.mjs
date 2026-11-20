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
            term: "You have found a good subtitle translation",
            detail:
              "[Meowcal Sub 2](@compare) automatically aligns existing subtitles to your video. You keep the translation its creator worked on, with the lines following playback.",
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
              "A short pause gives you time to read. If a good subtitle translation is available, [Sub 2](@compare) can show those existing lines in sync with the video.",
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
            body: "Your everyday translator, or existing subtitles synced to the film.",
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
      "Meowcal Sub is your go-to for on-screen subtitle translation. Sub 2 automatically syncs existing translations made for your film or series. Compare both Windows apps.",
    name: "Meowcal Sub or Sub 2: which should you use?",
    lead: "Meowcal Sub is the versatile, easy choice for everyday watching: select the subtitles on screen and read the translation. When a film or series already has a good subtitle translation, Sub 2 automatically aligns it to playback so you can enjoy the work its creator put into it.",
    sections: [
      {
        type: "pairs",
        id: "choose",
        heading: "Choose by what you want to do",
        termLabel: "Your goal",
        detailLabel: "Start here",
        items: [
          {
            term: "An easy subtitle translator for everyday watching",
            detail:
              "[Meowcal Sub](@sub1). A versatile starting point: select the words on screen and read the translation. No subtitle file, account, or API key needed.",
          },
          {
            term: "Watch a film or series with good subtitles already available",
            detail:
              "[Meowcal Sub 2](@sub2). Choose an existing translation made for the title and let the app align it to playback. A good fit for films and series with an established subtitle community.",
          },
          {
            term: "The subtitles you want are out of sync",
            detail:
              "[Meowcal Sub 2](@sub2). Its matching system uses the original subtitles visible in the video to find the playback position and line up your chosen translation.",
          },
          {
            term: "Watch together on two computers",
            detail:
              "[MeowWatch](https://github.com/PeterShanxin/MeowWatch) synchronizes playback and adds floating chat.",
          },
        ],
      },
      {
        type: "faq",
        id: "questions",
        heading: "Common choices",
        items: [
          {
            q: "Is Sub 2 an upgrade I need to install?",
            a: "Both apps run independently. Keep Sub 1 as your everyday translator and choose Sub 2 when you want to watch with an existing subtitle translation.",
          },
          {
            q: "Which one should I try first?",
            a: "Not sure where to start? Try [Meowcal Sub](@hardcoded): select the subtitles on screen and read the translation. If your film or series already has a translation you like, choose Sub 2 to watch with those subtitles.",
          },
          {
            q: "Can Sub 2 follow a video with no visible subtitles?",
            a: "Sub 2 needs readable captions to find your place. Turn them on in your player. If you only have a translated subtitle file, the app translates several on-screen lines to locate the scene in that file.",
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
            body: "Watch with existing translations, automatically synced to playback.",
            href: "@sub2",
          },
        ],
      },
    ],
  },
};
