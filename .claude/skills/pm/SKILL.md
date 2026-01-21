# PM - PRD & Requirements

Clarify ambiguous requirements into an executable PRD.

**Blocking Questions (max 10, only ask what's needed):**
1. Target users + core pain points
2. Success metrics (north star + 1-3 supporting metrics)
3. Scope & non-goals (MVP/v1 split)
4. Key user journeys (happy path + failure + edge cases)
5. Platform & permissions (Web/desktop/mobile, auth model)
6. Data & integrations (sources, storage, APIs, webhooks)
7. Quality & compliance (security, privacy/PII, performance, observability)

**Rules:**
- No guessing. If info missing, ask or explicitly list "assumptions + risks"
- Max 2 rounds of questions, max 10 per round, only ask blockers
- If user says "don't ask much": output MVP PRD with explicit assumptions/open_questions

**Output PRD:**
- Title + one-line summary
- Background & problem
- Goals & non-goals
- Personas & use cases
- User stories (each with acceptance criteria)
- Key flows (text steps or Mermaid)
- Feature list (Must/Should/Could)
- Data model draft (entities, fields, relationships)
- API draft (if applicable)
- Permissions & risk controls
- Analytics & metrics
- Failure modes & edge cases
- Milestones & version splits
- Risks & open items

**HANDOFF.v1:**
```yaml
handoff_version: "1"
from_skill: "gold-pm"
deliverable: "prd"
summary:
  - "<5-10 bullets: downstream must know>"
ui_brief:
  pages: ["<page1>", "<page2>"]
  key_flows: ["<flow1>", "<flow2>"]
  states: ["loading", "empty", "error", "permission-denied"]
tech_brief:
  modules: ["<module1>", "<module2>"]
  constraints: ["<constraint1>"]
  risks: ["<risk1>"]
test_brief:
  p0_tests: ["<case1>", "<case2>"]
assumptions: ["<assumption1>"]
open_questions: ["<question1>"]
recommended_next: ["ui-designer", "prd-tech-lead"]
```
