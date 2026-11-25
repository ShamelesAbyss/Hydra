# Contributing to Project Leviathan

Project Leviathan is Hydra. **main is the only active authoritative branch**.
Work directly on main; target main for review. Use main for builds, integrations,
documentation and releases. Do not create additional branches unless explicitly
requested by the repository owner.

`archive/dead-code-hydra-1.6.2` is retired historical code: do not use or develop it.
Existing release tags are immutable recovery points. Do not rewrite history or
replace published assets. Engineering prereleases are not stable-live certification.

Read PROJECT_STATUS.md, TEST_RESULTS.md and IMPLEMENTATION_PLAN.md for the exact
continuation point. Preserve unresolved execution/accounting holds and unknown
costs. Use offline fixtures and loopback transports for development verification.
Never enable real-money trading or send signed exchange requests during development.
Keep credentials and local databases out of Git.

Before publishing a coherent milestone: format, run the relevant complete tests,
build CLI and native desktop targets, audit dependencies, update documentation,
commit and push main, verify GitHub CI, and publish versioned verified binaries
with checksums. Research candidates do not automatically enter live execution.
