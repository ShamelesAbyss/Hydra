# HYDRA — Project Leviathan 1.7.0-alpha.24

GENESIS resumable generation checkpoints and native step controls.
Engineering prerelease on main; live-trading readiness remains incomplete.

## Implemented

- Share full/stepped search with exact random state, candidates, parents and history.
- Bind checkpoints to dataset, normalized strategy config, options and executable
  version; exclude wallet/account/runtime paths. Add state corruption checksum,
  structural/resource validation and JSON float-roundtrip fidelity.
- Publish synced bytes by an atomic non-replacing hard link into a new destination.
  Existing checkpoints are never overwritten; unsupported filesystems fail visibly.
- Add offline `optimize-step` and native complete-generation step/input/output
  controls with saved-state status and JSON copy. Normal full search remains.
- Preserve every execution/financial hold and the exact final memorial dedication.

Five targeted core tests cover serialized resume parity, completed-state stability,
context/corruption/schema refusal, secret exclusion/map ordering and immutable file
publication. Real CLI regression compares resumed/full results and preserves the
prior checkpoint with an invalid key/live flag but no wallet or engine storage.
Full test/audit/build/package checks precede publication; no signed requests or
real-money trading were activated.

## Remaining limits

Checkpoint hashes detect corruption, not authenticity or financial truth. State is
build-specific and resumable only at generation boundaries. Hard-link support is
required; power-loss directory durability depends on the filesystem. Candidate OOS,
full train/select/test walk-forward, NSGA-II and GPU remain outstanding. All original
Oracle model limitations remain. No major milestone is fully certified.
