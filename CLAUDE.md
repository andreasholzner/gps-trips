# Ruflo — Claude Code Configuration

## Rules

- Do what has been asked; nothing more, nothing less
- Be consise. Never report already fixed intermediate mistakes or adherence to rules
- NEVER create files unless absolutely necessary — prefer editing existing files
- NEVER create documentation files unless explicitly requested
- NEVER save working files or tests to root — use `/src`, `/tests`, `/docs`, `/config`, `/scripts`
- ALWAYS read a file before editing it
- Use the current file contents as ground truth, ignore earlier versions in the conversation.
- NEVER commit secrets, credentials, or .env files
- NEVER add a `Co-Authored-By` trailer to user commits unless this project's `.claude/settings.json` has `attribution.commit` set (#2078). The Claude Code Bash tool may suggest one in its default commit-message template — ignore it. `Co-Authored-By` is semantic authorship attribution under git/GitHub convention; the tool is the facilitator, not a co-author.
- Keep files under 500 lines where practical; 650 lines is a hard cap
- Validate input at system boundaries
- The ADRs for this project are maintained at `/docs/adr`. Always follow them.
- Follow TDD also for fixing issues found during code review.
- NEVER update `docs/initial_plan.md` — it is a frozen historical snapshot. Record design, scope, schema, or API changes in the living docs instead: `docs/requirements.md` (user stories), `docs/adr/` (decisions), `docs/architecture.md` (C4 diagrams). See the banner atop the file.
- ALWAYS run tests after code changes
- ALWAYS verify build succeeds before committing

## Code Review Focus

Every code review (manual or via `/code-review`) must check:
- **Correctness** — logic bugs, edge cases, error handling
- **Test coverage** — every user story's acceptance criteria has a corresponding test (see ADR-0012)
- **Readability** — naming, comments only where non-obvious, consistent with surrounding code
- **Rust conventions** — idiomatic error handling, ownership, no unnecessary clones/allocations, `clippy`-clean

## Swarm Coordination

Multi-agent orchestration, agent comms patterns, routing tables, and the claude-flow memory/hooks
workflow live in the `ruflo-swarm-coordination` skill — invoke it when you need them.
