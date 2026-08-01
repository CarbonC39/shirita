# Shirita documentation

This index separates current documentation from historical design artifacts. When documents disagree, current product and direction documents take precedence over archived milestone plans.

## Current guidance

- [Project README](../README.md) — public overview, current capabilities, setup, and development commands
- [Product direction](../PRODUCT.md) — product identity, commitments, non-goals, and decision boundary
- [Current development direction](current-direction.md) — active cleanup scope and sequencing
- [Phase 1 implementation plan](plans/phase-1-chat-correctness.md) — proposed task-by-task plan for chat correctness and recovery
- [Phase 2 implementation plan](plans/phase-2-remove-st-compatibility.md) — proposed deletion boundary and task sequence for removing SillyTavern compatibility
- [Architecture](../ARCHITECTURE.md) — current implementation architecture; update it alongside structural code changes

## Reference

- [Provider configuration](providers.md) — supported providers, environment variables, and UI settings
- [Deployment](deploy.md) — self-hosted deployment and operational notes
- [Module documentation](module-docs.md) — generated implementation survey; useful for navigation but not normative
- [Archived known gaps](archive/unimplemented-and-gaps.md) — dated inventory retained for historical context; validate every item against current code

## Historical archive

The former `docs/superpowers` directory is preserved at [`docs/archive/superpowers/`](archive/superpowers/). It contains milestone specs and implementation plans produced during the initial M0–M9 build-out.

These files explain how the current system evolved, but they are semi-deprecated:

- they are not the active roadmap;
- completion markers are historical, not current priorities;
- proposed behavior may have been superseded by later code or current direction;
- new work should not be added there.

Use the archive for archaeology and migration context only.
