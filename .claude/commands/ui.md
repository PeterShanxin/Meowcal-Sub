# UI Designer - UI Spec & Image Prompts

Convert a PRD into buildable UI specs with image generation prompts.

**Input Required:** PRD or at minimum: target users, key flows, page list, priority, state requirements.
If info missing, ask max 6 questions then proceed.

**Output (must include all):**
1. Design direction (3-6 keywords) + information architecture/navigation
2. MVP page list
3. Per-page layout (sections, main CTA, key components)
4. State design: loading / empty / error / permission-denied
5. Design system: color roles, typography scale, spacing grid, component specs
6. Image prompt pack:
   - A. Page list & descriptions (Chinese)
   - B. Master prompt (English)
   - C. Per-screen additions (English)
   - D. Negative prompt (English)

**Constraints:**
- Default Web SaaS style: high readability, implementable, unified design language
- Avoid 3D/complex animations
- Avoid human faces/hands/watermarks/garbled text

**HANDOFF.v1:**
```yaml
handoff_version: "1"
from_skill: "ui-designer"
deliverable: "ui-spec"
pages: ["<page1>", "<page2>"]
components_reusable: ["<component1>", "<component2>"]
states: ["loading", "empty", "error", "permission-denied"]
design_system_notes:
  - "<color roles + typography + spacing>"
assumptions: ["<assumption1>"]
open_questions: ["<question1>"]
recommended_next: ["prd-tech-lead", "a-plus-fixer"]
```
