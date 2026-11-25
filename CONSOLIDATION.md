# Project Leviathan repository consolidation

Project Leviathan is Hydra. **main is the only active authoritative branch**.
This promotion does not establish stable-live certification.

## Preserved history

- Original Hydra 1.6.2 base: `5720ecde7565583e85dde3ebf48acafa71af35e0`.
- Retired archive: [archive/dead-code-hydra-1.6.2](https://github.com/ShamelesAbyss/Hydra/tree/archive/dead-code-hydra-1.6.2), warning commit `574398470e345317e6c9b1002012e6c11b5cf991`.
- Source and presentation were joined with merge `177ef2bf2810e69183353bd270d7d5b635811390`, then fast-forwarded to main. No history was rewritten.
- Exact local residual-proof work (`4005789`) and flagship presentation (`6772341`) are ancestors of main. All prior release tags remain accessible.
- All 41 existing release download URLs were checked successfully. Alpha.8 has five published Windows/source/Linux/checksum assets with matching digests.

## Verification

- Verified main source: `fbb52619a1328533062aa7fe0a8b3bf5b4312223`.
- [All four main CI jobs passed](https://github.com/ShamelesAbyss/Hydra/actions/runs/45581670769): Windows, Linux, native desktop and dependency audit.
- Full offline desktop-feature suite: **267 tests pass**; deterministic causal regression also passes.
- Local Windows CLI and cockpit release builds pass, with offline version/configuration smoke checks.
- Formatting and Clippy complete with warnings. Fresh dependency audit: **0 vulnerability entries, 1 Chrono maintenance warning**, without advisory ignores.
- Lockfile SHA-256: `f3bf59fd2cf33ba25cd34d66d1eea760cc19b94faa49f35ddea129d149dc53a7`.
- GitHub default branch is main; there were no open pull requests to retarget. Workflows, badges, installation, contribution guidance and future-agent rules point to main.
- All ten SVGs parse; asset/supporting-document paths pass checks. GitHub Markdown renders in dark/light previews without broken images or horizontal overflow. The live default README loads all 20 images.

## Engineering continuation

Continue on main at the exact post-alpha.8 residual-proof bridge. Add runtime-specific protective-close regressions, then publish the next verified prerelease. Complete financial settlement, broader recovery, fixed-point position arithmetic and live-readiness work remain open. Do not activate real-money trading during development.

The redundant remote development branch and both local development/presentation branches were removed after ancestry and CI verification. Remote branch inventory is now **main** plus the retired archival branch only. Subsequent engineering proceeds directly on main.
