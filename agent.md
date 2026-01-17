# Meowcal-Sub Agent Guide

本仓库自带一组可复用的 agent skills/playbooks（位于 `.agent/skills/`），用于需求澄清、UI 设计、技术方案、修复与审查。

## Skills 一览（仓库内）
- `a-plus-fixer`：复现与修复问题，补回归测试，稳健重构
- `gold-pm`：需求澄清并输出可执行 PRD
- `picky-reviewer`：发布门禁审查（需求一致性/正确性/安全/性能/测试）
- `prd-tech-lead`：从 PRD 产出架构/Backlog/上线方案
- `router-orchestrator`：自动选择 skill，必要时串联交付
- `ui-designer`：从 PRD 产出 UI 方案与生图提示词

## 如何触发（推荐）
在对话里直接点名技能（建议用英文 id，匹配更稳定）：
- `a-plus-fixer`
- `gold-pm`
- `picky-reviewer`
- `prd-tech-lead`
- `router-orchestrator`
- `ui-designer`

也可以直接用中文说“写 PRD / 做 UI / 出技术方案 / 修 bug / 做审查”，但点名更稳定。

如果你的 agent 不支持“skills 目录”的加载方式：把对应的 `.agent/skills/<name>/SKILL.md` 内容直接贴给它，让它按里面的流程执行。

## Repo 快速开发命令（Tauri）
OneDrive 目录可能导致 Cargo target 文件锁；推荐设置自定义 target dir：
```powershell
$env:CARGO_TARGET_DIR = "D:\\cargo-build"
npx tauri dev
```
也可使用 `dev-tauri.cmd` 一键启动（同样会设置 target dir）。
