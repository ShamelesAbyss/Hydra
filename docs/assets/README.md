# Presentation assets

All SVG artwork is original repository artwork, distributed under the project's
MIT license. Each SVG includes a descriptive title and a self-contained dark
background for legibility in both GitHub themes. No scripts, remote fonts,
external SVG resources or foreignObject elements are used.

## Native captures

- `cockpit-alpha4.png`: actual native command center, empty first-launch state.
- `cockpit-local-history.png`: actual cockpit after offline empty database setup; no account records exist.
- `oracle-workspace.png`: actual native Oracle input workspace, before any dataset.
- `oracle-workspace-detail.png`: crop of that workspace; no content is changed.

Captures render `hydra::cockpit::Cockpit` through eframe/glow in a temporary
capture harness. The harness waits for layout/render stabilization, requests an
egui framebuffer screenshot and exits. The Oracle capture selects the real
navigation entry through injected application input. No account, credentials,
exchange connection, trades or generated performance data are involved.
The primary command-center and Oracle captures identify the alpha.9 build. The
Oracle detail is a pixel-preserving crop of that same capture (205,72)-(1440,284).
`cockpit-alpha4.png` preserves the earlier first-launch capture. These are actual
application renders, not design mockups.

The dark banner represents Hydra's seven logical subsystem boundaries; it is
conceptual branding, not telemetry or a measured computational result.

`hunter-workspace.png` is an actual alpha.21 eframe/glow native framebuffer capture.
Alt+1 selects the real HUNTER workspace through application input. No database,
market snapshot, credentials or exchange connection is supplied; the empty state
explicitly shows that no observations or scores are inferred.

`oracle-validation-workspace.png` is an actual alpha.22 native eframe/glow capture.
Application input opens ORACLE and its held-out-validation option. No candle data,
performance results, credentials or account session is supplied.

`cognition-workspace.png` is the actual alpha.23 native eframe/glow framebuffer.
Alt+3 selects COGNITION through application input. No dataset, account session or
outcome report is loaded; no statistics or trading performance are fabricated.

`cognition-synthetic-workspace.png` is an actual alpha.23 native capture after real
application input loads `cognition-synthetic-render-fixture.json` and evaluates it.
The 60 deterministic, explicitly simulated synthetic outcomes test interval/shadow
rendering only. They are not market data, performance, a benchmark or a profitability
claim. No account/credentials/exchange session is involved. The empty capture is
retained separately; all chart values are computed by the actual research kernel.
