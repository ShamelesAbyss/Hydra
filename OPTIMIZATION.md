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
