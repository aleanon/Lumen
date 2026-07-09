## Committing

commit with a clear explanation of what has been done for every task or subtask completed

## Doc currency (binding, adopted 2026-07-09 — plan D0.7)

Any commit that changes public behavior must update, **in the same commit**:
1. the affected spec section in `.ai_docs/02–05` (remove the *planned*
   marker if the change implements it; add one if it defers something);
2. the matching checkbox/note in `.ai_docs/06-task-graph.md` (◐/✗/☑);
3. any `.claude/skills/` table or snippet the change invalidates (the
   `styling-lss` property table and the `verifying-apps` method table are
   the usual suspects).

Rationale: the 2026-07 audit (`docs/review-docs-vs-code-2026-07.md`) found
the specs had drifted ~30% from the code because docs were updated in
separate passes. Docs describe reality, not intent; intent lives in
`docs/plan-remediation-2026-07.md` and the backlog.
