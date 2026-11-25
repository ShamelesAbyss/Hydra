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
The interface identifies itself as 1.7.0-alpha.4; capture occurred during the
subsequent recovery/dependency development checkpoint. These are not design mockups.

The dark banner represents Hydra's seven logical subsystem boundaries; it is
conceptual branding, not telemetry or a measured computational result.
