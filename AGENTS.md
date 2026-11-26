# Project Leviathan engineering guidance

Project Leviathan is Hydra. **main is the only active authoritative branch**.
Use main for development, pull-request targets, documentation and releases.
Work directly on main. Do not create additional branches unless the user
explicitly requests one. The retired `archive/dead-code-hydra-1.6.2` branch must never be
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

Preserve the owner's permanent memorial dedication verbatim as the final section
of the root README.md. The exact text is in .cursor/rules/permanent-memorial.mdc.
Never shorten or remove it during documentation edits, reorganizations or releases.
Nothing may follow the dedication except a trailing newline.


At the start of every engineering cycle reassess all seven major milestones in
MILESTONE_MATRIX.md. Update its implemented/verified/outstanding/blocker/deferred/
next-action entries at every meaningful release. Alpha release counts are not
major milestone completion. When successive checkpoints focus on one head, prefer
independent acceptance work elsewhere over nonblocking refinements. Preserve every
financial safeguard; defer live certification rather than blocking offline research
or read-only cockpit work. Never put material after the README memorial.

## Active owner sequence: Project Manifestation

Finish alpha.25, then suspend the normal engineering backlog for the unified native
seven-headed Leviathan visual campaign in VISUAL_CAMPAIGN.md. Preserve every financial
safeguard and the memorial. After completing/publishing that visual campaign, provide
its comprehensive handoff and pause for explicit owner authorization before returning
to the normal engineering queue. Do not interpret earlier continuous-work guidance as
permission to bypass this owner-requested post-campaign pause.

Release documentation uses repository-owned SVG status badges. Update release,
offline-test and dependency-audit values from verified evidence for each release;
keep the CI badge dynamic. Do not let a static badge imply unverified success.
