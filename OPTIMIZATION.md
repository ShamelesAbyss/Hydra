# Genesis bounded CPU research

Genesis evaluates momentum parameters through actual Oracle simulations. The
search uses a deterministic seed, crossover, mutation, global candidate
deduplication and Pareto filtering across net return, drawdown and fee burden.

```text
hydra --config config/strategies.toml optimize --candles historical-candles.json
hydra --config config/strategies.toml optimize --candles historical-candles.json --options search-options.json
```

SearchOptions contains seed, population, generations, max_evaluations, mutation_bps,
parameter bounds and replay options. Bounds cover fast/slow EMA periods, ATR
period and minimum ATR percentage. The default budget evaluates at most 32
candidates. Validation enforces parameter ordering, available lookback and global
simulation/Pareto work bounds.

Reports identify the algorithm as seeded-pareto-parent-mutation-crossover-v1 and
include candidates, objectives, generation progress, options and dataset range.
Results are research proposals. The command does not write engine configuration
or deploy candidates. Out-of-sample validation, resumable checkpoints, NSGA-II
crowding, GPU evaluation and portfolio scenarios remain open implementation work.

## Resumable generation checkpoints (alpha.24)

```powershell
.\hydra.exe --config config/strategies.toml optimize-step --candles candles.json --options search.json --generations 1 --output generation-1.json
.\hydra.exe --config config/strategies.toml optimize-step --candles candles.json --options search.json --generations 1 --checkpoint generation-1.json --output generation-2.json
```

GENESIS in the native cockpit also offers **Resumable generation steps** with input,
new output and generation-count controls. A blank input starts a new search. Reuse
the same dataset, strategy configuration and full search options for every step;
choose a new output filename each time. The normal full-search command remains.

Each complete generation retains random stream state, all evaluated candidates,
parent frontier and generation history. Resumption does not replay completed fitness
evaluations; serialized step-by-step output matches uninterrupted deterministic
search in regression tests. Reports mark whether search termination is complete.
Generation/evaluation/uniqueness/work budgets remain enforced.

The context SHA3-256 binds the complete candle data, normalized strategy settings,
search options and executable version. Per-symbol map order is normalized. Wallet
keys, account identity and unrelated runtime paths are excluded. Checkpoints are
build-specific, not a cross-version migration format. State carries a separate
SHA3-256 corruption checksum and bounded structural validation. Neither hash is an
authenticity proof: do not import an untrusted file as verified research evidence.
No checkpoint can certify financial outcomes, authorize deployment or change risk.

Complete JSON bytes are written to a new temporary file and synced, then atomically
hard-linked into a previously nonexistent destination. Existing files are never
overwritten and no weaker fallback is used; the destination directory must exist
on a filesystem supporting hard links. Temporary links are removed after completion.
The previous checkpoint remains usable if a later step fails. Sudden power-loss
directory durability is filesystem-dependent; retain earlier immutable checkpoints.
The reader refuses truncated/corrupt/mismatched state. Output is limited to 8 MiB.

Checkpointing occurs at explicit generation-step boundaries, not during a single
candidate replay. Candidate out-of-sample assessment, full train/select/test
walk-forward fitting, NSGA-II and GPU remain outstanding. The optimizer still uses
Oracle's incomplete candle execution model; all outputs remain simulated proposals.

## Frozen-frontier held-out assessment (alpha.25)

```powershell
.\hydra.exe --config config/strategies.toml assess-candidates --training training.json --held-out later-candles.json --options search.json --checkpoint completed-search.json --limit 4
```

In GENESIS enable **Assess frozen candidates / later held-out data**. The original
Candles JSON input is training data; supply a completed matching-build checkpoint,
later held-out candles and optional validation-options JSON. Use the same strategy
config/search options that produced the checkpoint. Checkpoints remain build-specific;
create the training checkpoint with the matching executable. Nothing is deployed.

The completed training frontier is frozen and its canonical parameter-order prefix
is chosen **before** any held-out evaluation. Reports state the number left unassessed.
Every held-out history/test bar must be chronologically later than all training bars;
reuse of warmup bars from training is deliberately refused. Parameters are fixed,
strategy state/capital reset per fold and original explicit cost scenarios apply.
No held-out outcome ranks or automatically selects a deployment candidate.

At most 16 candidates, 64 combined candidate/fold/scenario calls, 100 million total
history-work units and one million retained equity/trade entries. Candidate settings
share the same momentum configuration helper as training. CLI/native reports retain
full per-candidate fold/scenario reports, training context and chronology boundaries.

Chronology/context checks cannot prove that an operator never inspected or tuned
against the dataset. Checkpoint state is caller-supplied and unauthenticated; its
training objectives are simulated research state, not certified financial evidence.
Multiple comparisons/selection bias and all Oracle funding/queue/latency/partial-fill/
liquidation/portfolio omissions remain. This is candidate assessment, not full
train/select/test walk-forward fitting, significance certification or live readiness.
