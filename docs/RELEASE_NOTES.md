# Release notes

Each release has hand-written notes in `docs/releases/v<version>.md`. The file
becomes the GitHub release body, the updater's notes in `latest.json`, and the
source of the Microsoft Store "What's new" text. Readers are users deciding
whether to update, so the notes say what changed for them, not how it was built.

## Rules the release tooling enforces

- The first line is exactly `# Meowcal Sub v<version>`.
- The file has a `## Fixes and improvements` section of `- ` bullets, with no
  icon in that heading. `scripts/store-release-assets.mjs` builds the Store text
  from those bullets, in order, until 1500 characters; put the most important
  first.
- The published release body must equal the file at the tagged commit, or the
  Store sync refuses the release. Finish the notes before `release.yml` runs.
  Editing a published body afterwards leaves that tag unable to pass Store
  verification again.

## Layout

1. **Title**, as above.
2. **Lead.** One bold sentence naming the biggest change in the user's terms,
   then one plain sentence on why it matters.
3. **Proof table**, when the headline has numbers: at most four rows comparing
   before and after, measured the same way, with the sample named in the header.
4. **`## ✨ Highlights`**: two to five bullets. Each starts with one icon and a
   bold name, then one or two sentences of what the user gets. Link the issue.
5. **`## Fixes and improvements`**: one sentence per user-visible change.
6. **`## ⚠️ Known limitations`**: what a user may hit and what to do about it.
7. **`## 📦 Download`**: a table from kind of PC to installer, the bundled Core
   version, and `SHA256SUMS.txt`.
8. **How it was tested**, folded in `<details>`: the evidence and its limits.

## Style

- Name controls as they appear in the app, for example **Settings → Profile**.
- Use icons only on section headings and highlight bullets, from this set:
  🎯 accuracy, ⚡ speed, 🎛️ settings, 🛟 reliability, 🖥️ windows and overlay,
  🌐 languages, 🔒 privacy and security.
- Leave out dependency updates, CI, packaging and refactors unless a user
  notices them.
- Every number names its sample. Do not claim "never", "always" or "fully"
  unless a measurement shows it.
- Keep the notes short enough to read in a minute; detail belongs in the linked
  issue or evidence.

[v0.9.0](releases/v0.9.0.md) is the reference example.
