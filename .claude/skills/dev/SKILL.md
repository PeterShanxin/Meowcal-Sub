# Dev Orchestrator

A unified skill that auto-routes requests to the appropriate workflow: PM, UI Design, Tech Lead, Fixer, or Reviewer. Supports chaining workflows via HANDOFF.v1.

## Usage
```
/dev <your request>
```

Examples:
- `/dev 写PRD` → PM workflow
- `/dev UI设计` → UI Designer workflow
- `/dev 怎么实现这个功能` → Tech Lead workflow
- `/dev 修bug` → Fixer workflow
- `/dev 上线前审查` → Reviewer workflow

---

## STEP 1: Route the Request

Identify the primary intent (choose exactly 1):

| Intent | Triggers | Workflow |
|--------|----------|----------|
| **PM** | PRD, requirements, scope, MVP, user stories, acceptance criteria, edge cases | → gold-pm |
| **UI** | page structure, IA, components, design system, mockups, image prompts | → ui-designer |
| **Tech** | architecture, stack, API, database, DevOps, CI/CD, security, release plan, tickets | → prd-tech-lead |
| **Fix** | bug fix, refactor, tests, performance issue, logs, reproduction, debugging | → a-plus-fixer |
| **Review** | review, audit, security/perf review, release readiness, checklist | → picky-reviewer |

**Routing Rules:**
1. If request clearly fits one category → single workflow
2. If request spans stages → chain workflows (e.g., PM → UI → Tech, or Fixer → Reviewer)
3. Max 6 clarifying questions at routing level. If user says "don't ask much", proceed with explicit assumptions.

**Output routing decision:**
```
Route: <workflow-name>
Mode: single | chain
Chain: <workflow1> → <workflow2> (if chain mode)
Rationale: <3-6 bullets>
```

Then execute the selected workflow(s) below.

---

## WORKFLOW: gold-pm (PRD & Requirements)

**Goal:** Turn ambiguous requirements into an executable PRD.

**Blocking Questions (max 10, only ask what's needed):**
1. Target users + core pain points
2. Success metrics (north star + 1-3 supporting metrics)
3. Scope & non-goals (MVP/v1 split)
4. Key user journeys (happy path + failure + edge cases)
5. Platform & permissions (Web/desktop/mobile, auth model)
6. Data & integrations (sources, storage, APIs, webhooks)
7. Quality & compliance (security, privacy/PII, performance, observability)

**Output PRD (strict format):**
- Title + one-line summary
- Background & problem
- Goals & non-goals
- Personas & use cases
- User stories (each with acceptance criteria)
- Key flows (text steps or Mermaid)
- Feature list (Must/Should/Could)
- Data model draft (entities, fields, relationships)
- API draft (if applicable: endpoint, auth, req/resp, error codes)
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

---

## WORKFLOW: ui-designer (UI Spec & Image Prompts)

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

---

## WORKFLOW: prd-tech-lead (Architecture & Implementation Plan)

**Rules:**
- Only ask blocking questions (max 8): platform, permissions, data sources, SLA, budget/team capacity
- Every key decision must include trade-offs (speed/cost/scalability)
- Output must be directly splittable into tickets

**Output:**
1. Tech stack recommendation + rationale
2. Architecture diagram (Mermaid) + module boundaries/dependencies
3. Data model + migration strategy
4. API contracts (auth, error codes, timeout/retry)
5. Security & release baseline (input validation, authz, secrets, rate limiting, log redaction)
6. Observability (logs/metrics/tracing)
7. CI/CD (lint/test/build/deploy) + rollback strategy
8. Milestones + Backlog (each item with acceptance criteria)
9. Risks & mitigations

**Safety:**
- No destructive commands (rm/del/format) unless user explicitly confirms with constrained path
- No secrets/PII in output

**HANDOFF.v1:**
```yaml
handoff_version: "1"
from_skill: "prd-tech-lead"
deliverable: "engineering-plan"
architecture_modules: ["<module1>", "<module2>"]
api_endpoints: ["<GET /x>", "<POST /y>"]
ops: ["ci-cd", "rollback", "observability"]
top_risks: ["<risk1>"]
backlog_highlights: ["<ticket1>", "<ticket2>"]
assumptions: ["<assumption1>"]
open_questions: ["<question1>"]
recommended_next: ["a-plus-fixer", "picky-reviewer", "ui-designer"]
```

---

## WORKFLOW: a-plus-fixer (Debug, Fix, Test, Refactor)

**Principles:**
- Reproduce before fixing. No repro steps? Create minimal reproduction first.
- Every fix must include: test or automated verification.
- Small changes, avoid mixing unrelated refactors.

**Questions (max 8):**
- Expected vs actual behavior
- Minimal reproduction steps
- Environment (OS/version/runtime/browser)
- Error messages/logs/screenshots
- Recent changes (commit/PR/feature flag)
- Is breaking change allowed?

**Output:**
1. Diagnosis: possible root causes (ranked by probability) + verification plan
2. Fix: hotfix vs clean fix + risk + rollback
3. Tests: new test suggestions + regression checklist
4. Refactor (if applicable): boundaries, benefits, migration steps

**Safety:**
- No destructive commands by default
- File deletion/overwrite requires explicit confirmation with exact paths

**HANDOFF.v1:**
```yaml
handoff_version: "1"
from_skill: "a-plus-fixer"
deliverable: "fix"
repro_steps: ["<step1>", "<step2>"]
root_cause: "<one-liner>"
fix_summary: "<one-liner>"
verification:
  tests_added: ["<test1>"]
  manual_checks: ["<check1>"]
risks: ["<risk1>"]
recommended_next: ["picky-reviewer", "prd-tech-lead"]
```

---

## WORKFLOW: picky-reviewer (Release Gate Review)

**Inputs Required:**
- PRD/goals and acceptance criteria
- Code diff / repo state
- How to run + how to test

**Must Cover:**
1. Requirement alignment
2. Correctness (edge cases, error handling, idempotency)
3. Security & privacy (validation, injection, authz, PII redaction, secrets)
4. Performance & reliability (timeout, retry, cache, graceful degradation)
5. Maintainability (module boundaries, complexity, dependency risks)
6. Testing & observability (coverage matches risk, debuggability)
7. UX & accessibility (empty states, error messages, keyboard navigable, contrast)

**Output (strict):**
- **Verdict:** shippable | needs-fixes | blocked
- **Findings:** P0/P1/P2, each must include: evidence → impact → fix → verification
- **Action checklist** (by priority)

**HANDOFF.v1:**
```yaml
handoff_version: "1"
from_skill: "picky-reviewer"
deliverable: "review"
verdict: "shippable|needs-fixes|blocked"
p0: ["<item1>"]
p1: ["<item1>"]
p2: ["<item1>"]
top_risks: ["<risk1>"]
recommended_next: ["a-plus-fixer", "gold-pm", "prd-tech-lead"]
```

---

## Chaining Workflows

When in chain mode, after completing each workflow:
1. Output that workflow's HANDOFF.v1
2. Use the handoff as input context for the next workflow
3. Continue until chain is complete

**Common chains:**
- PM → UI → Tech Lead (full feature development)
- PM → Tech Lead (backend-focused feature)
- Fixer → Reviewer (bug fix validation)
- PM → Tech Lead → Reviewer (pre-release planning)
