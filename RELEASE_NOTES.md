# HYDRA — Project Leviathan 1.7.0-alpha.22

ORACLE held-out cost scenarios and native comparison workspace.
Engineering prerelease on main; live-trading readiness remains incomplete.

## Implemented

- Add bounded fixed-configuration rolling held-out evaluation, disjoint test windows,
  prior indicator history, fresh strategy state and independently reset capital.
- Compare explicit cost scenarios with exact range/timestamp metadata, complete
  replay results and arithmetic summaries that never imply portfolio compounding.
- Add offline `validate-research` and native scenario/fold equity/trade panels.
- Preserve HUNTER Radar, Genesis search, engine/TUI and all execution/financial holds.
- Update the seven-major-milestone acceptance matrix; no major is fully certified.

Six new core tests cover deterministic ranges, state resets, adverse costs,
future-data isolation and aggregate resource/chronology/schema rejection. The real
research CLI isolation test now covers validation with an invalid key and live
flag, without creating live storage. Full release checks precede publication.

## Limits and next work

No parameters are fitted, trained or selected. A caller may already have inspected
the dataset; the evaluator cannot certify unseen-data provenance. Preserve exact
config and dataset for reproduction. Funding, partial fills, queue/latency and
multi-instrument simulation remain unimplemented. GENESIS resumability/OOS and
independent FANG/COGNITION/AEGIS research are next. No real-money trading activated.
