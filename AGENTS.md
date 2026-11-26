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

## Permanent Definition of Done & Release Publication Protocol (Mandatory)

A completed Project Leviathan development milestone is NEVER complete until ordinary users can actually download, double-click, and use the application from GitHub Releases. Every coherent, verified implementation milestone MUST produce ALL of the following:

1. Implementation and full regression testing (offline `desktop-wgpu` + headless tests).
2. Successful compilation (Windows WGPU standalone graphical executable, headless/terminal CLI, Linux headless) and native graphical launch verification.
3. An incremented application version in `Cargo.toml` (and matching `Cargo.lock`).
4. Updated `CHANGELOG.md`, `RELEASE_NOTES.md`, `PROJECT_STATUS.md`, `TEST_RESULTS.md`, and `MILESTONE_MATRIX.md`.
5. Updated `README.md` top download links, version badges, and release links (preserving Ashlan Haley Rapponotti's exact verbatim final memorial).
6. A committed and pushed `main` branch.
7. A corresponding exact Git tag (`v<version>`).
8. Passing exact-tag GitHub Actions release verification (`.github/workflows/release.yml`: tag/version check, memorial check, fmt, Clippy, desktop-wgpu + headless tests, `cargo audit`, smoke tests).
9. A visible, non-draft GitHub Release at `https://github.com/ShamelesAbyss/Hydra/releases` with comprehensive release notes directly on the release page.
10. Verified downloadable assets including the primary standalone Windows executable **`HYDRA-Leviathan-Windows-x64.exe`**, advanced Windows portable ZIP (`HYDRA-Leviathan-Windows-x64.exe` + `hydra-cockpit.exe` + `hydra.exe`), Linux headless `tar.gz`, exact-tag source ZIP, and SHA-256 checksum manifests (plus signed manifests when release-signing infrastructure is configured).
11. Verified public unauthenticated download accessibility and exact SHA-256 integrity.

A successful `git push` alone is NOT completion. A local build alone is NOT completion. A Git tag alone or CI workflow artifact alone is NOT completion. Ordinary users must never search through GitHub Actions artifacts to download Hydra. Always preserve truthful `alpha`, `beta`, `rc`, and `stable` classifications, never label unfinished engineering builds as stable, and never overwrite historical releases.

## Active owner directive: Project Awakening

Continue autonomously on `main` across all seven major milestones (HUNTER, FANG, COGNITION, AEGIS, NEXUS, ORACLE, GENESIS), the native **LEVIATHAN EVOLUTION** update manager, **LEVIATHAN TREASURY / THE OFFERING**, reliable autonomous infrastructure, and continuous visual integration in the single unified native cockpit. Preserve every financial safeguard, headless/Ratatui operation, and Ashlan Haley Rapponotti's permanent memorial dedication across the application, README, automated checks, distributed packages, and future update packages.

Release documentation uses repository-owned SVG status badges. Update release,
offline-test and dependency-audit values from verified evidence for each release;
keep the CI badge dynamic. Do not let a static badge imply unverified success.
