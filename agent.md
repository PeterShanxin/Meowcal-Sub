# Meowcal-Sub Agent Guide

这个仓库附带 3 个可复用的 agent skills/playbooks（见 `skills/`），用于：
- 需求澄清 + 输出 PRD（毒舌产品开发经理）
- 生成专业 UI 生图提示词（多页多层）（UI 提示词生成师）
- 从需求到交付的一体化开发（全能产品开发码农）

## Skills 一览（仓库内）
- `skills/dushe-product-manager`：毒舌产品开发经理（追问到你把需求说清楚，再给 PRD）
- `skills/ui-prompt-generator`：UI 提示词生成师（给专业生图模型的 prompts）
- `skills/all-round-product-engineer`：全能产品开发码农（方案+实现+测试+交付）

## 如何触发（推荐）
在对话里直接点名技能（推荐用英文 id，匹配更稳定）：
- `dushe-product-manager`
- `ui-prompt-generator`
- `all-round-product-engineer`

也可以直接用中文说“写 PRD / 生成 UI 生图提示词 / 把这个功能做出来”，但点名更稳定。

如果你的 agent 不支持“skills 目录”的加载方式：把对应的 `skills/<name>/SKILL.md` 内容直接贴给它，让它按里面的流程执行。

## Repo 快速开发命令（Tauri）
OneDrive 目录可能导致 Cargo target 文件锁；推荐设置自定义 target dir：
```powershell
$env:CARGO_TARGET_DIR = "D:\\cargo-build"
npx tauri dev
```
