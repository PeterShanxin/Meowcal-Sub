# Release notes

Each release has hand-written notes in `docs/releases/v<version>.md`. The file
becomes the GitHub release body and the
source of the Microsoft Store "What's new" text. Readers are users deciding
whether to update, so the notes say what changed for them, not how it was built.

The updater's `latest.json` contains at most three Highlights bullets, each
reduced to its bold name and first explanatory sentence. Releases without
Highlights use the first three Fixes bullets. The app displays these as a list
with **Full release notes ↗** linking to that version's GitHub release. The
same summary helper handles older manifests that contain the full release body.
Keep essential upgrade instructions in the first sentence of an early bullet.

## Rules the release tooling enforces

- The first line is exactly `# Meowcal Sub v<version>`.
- The file has a `## Fixes and improvements` section of `- ` bullets, with no
  icon in that heading, and may have a `## ✨ Highlights` section.
  `scripts/store-release-assets.mjs` builds the Store text from the Highlights
  bullets and then the Fixes bullets, in order, stopping at the first bullet
  that would pass 1500 characters; put the most important first. The Store text
  is plain, so the tooling drops each highlight's leading icon and every `**`.
- Store sync requires the published release body to equal the file at the tagged
  commit. Finish the notes before `release.yml` runs. Post-publication editorial
  corrections follow the process below.

## Layout

1. **Title**, as above.
2. **Lead.** One bold sentence naming the biggest change in the user's terms,
   then one plain sentence on why it matters.
3. **Proof table**, when the headline has numbers: at most four rows comparing
   before and after, measured the same way, with the sample named in the header.
4. **`## ✨ Highlights`**: two to five bullets. Each starts with one icon and a
   bold name, then one or two sentences of what the user gets. Make the first
   sentence useful on its own; the updater displays only that sentence and the
   name, capped at 220 characters each. Link the issue.
5. **`## Fixes and improvements`**: one sentence per user-visible change.
6. **`## ⚠️ Known limitations`**, only when needed: a concrete issue that affects
   the decision to update or requires a workaround. Omit the section otherwise.
7. **`## 📦 Download`**: a table from kind of PC to installer, the bundled Core
   version, and `SHA256SUMS.txt`.
8. **Testing evidence**, only when it substantiates a claim in the notes: link
   the relevant result. Keep routine CI, hardware coverage and manual-test
   details in the PR or evidence documents.

## Style

- Name controls as they appear in the app, for example **Settings → Profile**.
- Use icons only on section headings and highlight bullets, from this set:
  🎯 accuracy, ⚡ speed, 🎛️ settings, 🛟 reliability, 🖥️ windows and overlay,
  🌐 languages, 🔒 privacy and security.
- Leave out dependency updates, CI, packaging and refactors unless a user
  notices them.
- Product scope, unchanged dependencies and routine settings behavior are not
  known limitations. Keep operating details in the user guide; put installation
  notices beside the downloads.
- Do not append generic caveats, lists of what was not tested, or descriptions
  of the release-preparation PR. Include a qualification only when omitting it
  would make a specific claim misleading or hide an actionable problem.
- Every number names its sample. Do not claim "never", "always" or "fully"
  unless a measurement shows it.
- Keep the notes short enough to read in a minute; detail belongs in the linked
  issue or evidence.

[v0.9.0](releases/v0.9.0.md) is the reference example.

## Editorial corrections after publication

For an explicitly requested wording correction after successful Store sync,
update both language files through a documentation PR, then copy the reviewed
English file to the GitHub release body. Point its language link to the corrected
document on `main`. Record the original successful Store run in the PR.

Keep the release tag, installers and updater assets unchanged. This edits the
GitHub notes only; it does not update the Store submission. The Store verifier
still requires the original tagged body and will reject a submission rerun after
the correction. Do not rerun Store submission to apply an editorial change.
