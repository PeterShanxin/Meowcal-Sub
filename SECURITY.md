# Security policy

## Reporting a vulnerability

Do **not** open a public issue for a security vulnerability.

Report privately through GitHub's private vulnerability reporting:

<https://github.com/PeterShanxin/Meowcal-Sub/security/advisories/new>

If that form is unavailable, email the maintainer at shanxin@u.nus.edu.

Include:

- a description of the issue;
- steps to reproduce;
- affected versions if you know them.

Do not include captured subtitle text, screenshots of what you were watching,
or raw logs that contain them. This application reads text off the screen.
Timings, error codes, engine state, and file paths are the parts that help.

There is no bug bounty.

## What this project treats as sensitive

- updater signing keys and GitHub tokens;
- self-hosted runner registration and removal tokens;
- captured or translated subtitle text;
- model and runtime download hashes and URLs, which are supply-chain data.

## Supported versions

Security fixes target the current release on
[GitHub Releases](https://github.com/PeterShanxin/Meowcal-Sub/releases) and
the `main` branch. Older packaged versions are not separately maintained.
