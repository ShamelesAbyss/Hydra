# Project Leviathan engineering guidance

Project Leviathan is Hydra. **main is the only active authoritative branch**.
Use main for development, pull-request targets, documentation and releases.
Short-lived isolated branches are allowed; do not create a competing long-lived
implementation. The retired `archive/dead-code-hydra-1.6.2` branch must never be
used or developed for live trading. Preserve all history and existing release tags.

Read CONTRIBUTING.md, PROJECT_STATUS.md, TEST_RESULTS.md and IMPLEMENTATION_PLAN.md
before continuing work. Preserve local unfinished work and unresolved financial
or execution evidence. Do not infer missing costs, precision or settlement.

Development verification must not activate real-money trading, probe account
credentials, send signed exchange requests or transfer funds. Use offline fixtures
and loopback transports. Never commit secrets, local databases or build artifacts.

For coherent milestones, update accurate documentation, run formatting and relevant
tests/builds/audits, push main, verify exact-commit CI, and publish appropriately
classified executables with checksums. Promotion or publication is not stable-live
certification. Continue authorized engineering within the available session budget.
