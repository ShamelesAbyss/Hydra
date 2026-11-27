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
classified executables with checksums in GitHub Releases. Promotion or publication
is not stable-live certification. Continue authorized engineering within the
available session budget without stopping prematurely after a single release.

Preserve the owner's permanent memorial dedication verbatim as the final section
of the root README.md. The exact text is in `.cursor/rules/permanent-memorial.mdc`.
Never shorten or remove it during documentation edits, reorganizations or releases.
Nothing may follow the dedication except a trailing newline.

At the start of every engineering cycle reassess all seven major milestones in
`MILESTONE_MATRIX.md`. Update its implemented/verified/outstanding/blocker/deferred/
next-action entries at every meaningful release. Alpha release counts are not
major milestone completion. When successive checkpoints focus on one head, prefer
independent acceptance work elsewhere over nonblocking refinements. Preserve every
financial safeguard; defer live certification rather than blocking offline research
or read-only cockpit work. Never put material after the README memorial.

## Permanent GitHub Distribution Compliance (MANDATORY — ZERO EXCEPTIONS)

This policy is permanent and supersedes any conflicting historical instruction.
See `.cursor/rules/github-compliance.mdc` and `.github/scripts/verify-github-compliance.ps1`.

1. **NEVER include in `README.md` or any other repository documentation, generated page, or badge:**
   - Direct executable download links (`.exe`, `.apk`, `.msi`, `.dmg`).
   - Direct ZIP or archive download links (`.zip`, `.tar.gz`).
   - Direct checksum download links (`SHA256SUMS.txt`).
   - Download buttons or download badges linking directly to executable or archive assets.
   - Any `/releases/download/` or `/releases/latest/download/` link.
   - Any `browser_download_url` link.
   - Redirect services, URL shorteners, external download mirrors, download proxy services, or disguised/indirect artifact download mechanisms.
2. **Approved Distribution Method:**
   - All downloadable executables, portable archives, source archives, release notes, and SHA-256 checksums must remain **exclusively inside the repository's native GitHub Releases section**.
   - `README.md` may contain an ordinary informational link to the repository's native GitHub Releases page (`https://github.com/ShamelesAbyss/Hydra/releases`) using neutral wording:
     > "Compiled executables, portable packages, source archives, release notes and SHA-256 checksums are available through the repository's GitHub Releases section."
   - Informational badges (release version, CI status, offline test count, language, license, audit, platform support) are permitted only if they do not act as direct artifact download links.
3. **Locally Committed Artwork:**
   - Store all README banners, diagrams, icons, and screenshots under `docs/assets/` and reference them with exact case-sensitive repository-relative paths. Never rely on external image hosts or third-party asset proxies.
4. **Automated Enforcement:**
   - `./.github/scripts/verify-github-compliance.ps1` runs in both `.github/workflows/verify.yml` and `.github/workflows/release.yml` and must pass before every commit and release. Do not remove or weaken this check without explicit owner authorization and clarification from GitHub Support.

## Permanent Definition of Done & Release Publication Protocol (Mandatory)

A completed Project Leviathan development milestone is NEVER complete until ordinary users can locate and download verified binaries in the repository's native GitHub Releases section. Every coherent, verified implementation milestone MUST produce ALL of the following:

1. Implementation and full regression testing (offline `desktop-wgpu` + headless tests).
2. Successful compilation (Windows WGPU standalone graphical executable, headless/terminal CLI, Linux headless) and native graphical launch verification.
3. An incremented application version in `Cargo.toml` (and matching `Cargo.lock`).
4. Updated `CHANGELOG.md`, `RELEASE_NOTES.md`, `PROJECT_STATUS.md`, `TEST_RESULTS.md`, and `MILESTONE_MATRIX.md`.
5. Updated `README.md` informational badges, artwork/screenshots (exact-cased local `docs/assets/` paths), accurate release status, GitHub-compliant neutral Releases link, and Ashlan Haley Rapponotti's exact verbatim final memorial.
6. Passing `./.github/scripts/verify-memorial.ps1` and `./.github/scripts/verify-github-compliance.ps1`.
7. A committed and pushed `main` branch.
8. A corresponding exact Git tag (`v<version>`).
9. Passing exact-tag GitHub Actions release verification (`.github/workflows/release.yml`: tag/version check, memorial check, GitHub compliance check, fmt, Clippy, desktop-wgpu + headless tests, `cargo audit`, smoke tests).
10. A visible, non-draft GitHub Release in the repository's native Releases section with comprehensive release notes directly on the release page.
11. Verified downloadable assets attached to the GitHub Release: primary standalone Windows graphical executable **`HYDRA-Leviathan-Windows-x64.exe`**, advanced Windows portable ZIP (`HYDRA-Leviathan-Windows-x64.exe` + `hydra-cockpit.exe` + `hydra.exe`), Linux headless `tar.gz`, exact-tag source ZIP, and SHA-256 checksum manifests (plus signed manifests when release-signing infrastructure is configured).
12. Independent post-publish verification of asset integrity (SHA-256 match) and clean-profile standalone startup on Windows without configured wallet credentials (`DORMANT` default).

A successful `git push` alone is NOT completion. A local build alone is NOT completion. A Git tag alone or CI workflow artifact alone is NOT completion. Always preserve truthful `alpha`, `beta`, `rc`, and `stable` classifications, never label unfinished engineering builds as stable, and never overwrite historical releases.

## Active owner directive: Project Awakening

Continue autonomously on `main` across all seven major milestones (HUNTER, FANG, COGNITION, AEGIS, NEXUS, ORACLE, GENESIS), the native **LEVIATHAN EVOLUTION** update manager, **LEVIATHAN TREASURY / THE OFFERING**, reliable autonomous infrastructure, and continuous visual integration in the single unified native cockpit. Preserve every financial safeguard, headless/Ratatui operation, permanent GitHub compliance, and Ashlan Haley Rapponotti's permanent memorial dedication across the application, README, automated checks, distributed packages, and future update packages.

Release documentation uses repository-owned SVG status badges. Update release,
offline-test and dependency-audit values from verified evidence for each release;
keep the CI badge dynamic. Do not let a static badge imply unverified success.
