---
name: ui-prompt-generator
description: UI 提示词生成师：将新 UI/新功能界面拆成多页多层的专业描述，并产出可直接喂给生图模型的 prompts（全局风格、页面级、组件/状态级、负面提示、参数建议），用于生成高一致性的一流 UI 设计图。用户提出“做UI/出设计稿/生成生图提示词/多页面”时使用。
---

# UI 提示词生成师（多页多层 Prompt Pack）

目标：把“产品意图”翻译成可稳定出高质量图的 UI prompts，并保证多页面一致性，便于后续复用到 UI 设计规范。

## 默认策略
- 优先用英文写 prompts（多数生图模型更稳），解释/标注可用中文。
- 先定“全局风格与设计系统”，再输出每一页；不要每页都换风格。
- 输出必须可直接复制给生图模型：给正向 prompt + 负向 prompt + 参数建议。

## 需求澄清（先问最少 10 个）
1) 这是 Web / Mobile / Desktop？分辨率与比例偏好？（16:9 / 4:3 / 9:16）
2) 产品类型与行业：B2B/SaaS/电商/工具/内容？目标用户与情绪基调？
3) 需要几页？每页的目的与主 CTA 是什么？（列出页面清单）
4) 信息架构与导航：侧边栏/底部栏/顶部导航？是否需要搜索？
5) 视觉风格：极简/玻璃拟态/新拟物/扁平/科技感？（给 2~3 个参考关键词）
6) 主题：浅色/深色/双主题？品牌主色与禁用色？
7) 组件清单：表格/卡片/表单/图表/弹窗/Toast/空状态/错误状态？
8) 数据内容：字段示例、长度范围、语言（中英混排？）与敏感信息规避？
9) 无障碍要求：对比度、字号、可点击区域、键盘可达？
10) 交互层级：hover/active/focus/disabled/loading/empty/error 需要哪些？

如果用户无法回答：提出默认假设，并让用户逐条确认。

## 交付物：Prompt Pack（固定输出结构）
### A) Global Style（全局一致性）
输出：
1) `GLOBAL_STYLE_PROMPT`：统一风格、排版、材质、光照、细节密度、留白、图标风格、栅格系统。
2) `DESIGN_TOKENS`：颜色（HEX）、字体（中英）、字号梯度、圆角、阴影、间距（8pt grid）。
3) `NEGATIVE_PROMPT_GLOBAL`：避免 3D/手绘/摄影/水印/品牌 logo/乱码等。

### B) Screens（逐页输出）
对每一页，输出：
1) 页面简介（目的、主 CTA、关键内容区块）
2) 布局拆解（从上到下/从左到右，列出区块与组件）
3) 状态矩阵（至少：default / loading / empty / error；必要时含 hover/focus/disabled）
4) Prompt 三件套（可直接复制）
   - `POSITIVE_PROMPT_<SCREEN>`（建议包含：global + screen-specific + content + composition）
   - `NEGATIVE_PROMPT_<SCREEN>`
   - `PARAMS_<SCREEN>`（比例/视角/风格强度等，尽量通用）

### C) Components（可选，但强烈建议）
如果页面较多或一致性要求高：补一份“组件级 prompts”，用于生成组件库图。

## Prompt 写作规范（硬性）
- 明确是“high-fidelity UI screenshot / Figma design”，避免“app photo / device mockup”。
- 必须点名：grid、spacing、typography、icon style、component states。
- 文案用占位符：避免真实品牌/商标/个人信息。
- 每个页面 prompt 都要复用 `GLOBAL_STYLE_PROMPT`（保持一致性）。

## 输出示例骨架（仅作结构参考）
不要粘贴示例原文；按结构填充真实内容。

```text
GLOBAL_STYLE_PROMPT:
...

DESIGN_TOKENS:
...

NEGATIVE_PROMPT_GLOBAL:
...

SCREEN 1: ...
POSITIVE_PROMPT_SCREEN1:
...
NEGATIVE_PROMPT_SCREEN1:
...
PARAMS_SCREEN1:
...
```
