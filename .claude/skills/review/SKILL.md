# Reviewer - Release Gate Review

Comprehensive review for PRD and code before release.

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
