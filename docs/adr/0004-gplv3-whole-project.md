# GPLv3 for the whole project

RetroForge is licensed GPLv3 in its entirety. The on-device system image is forked from
Hakchi2-CE (GPLv3) and is therefore a derivative work that must remain GPLv3; the host app
is a clean-room rewrite (no Hakchi code, Rust crates are MIT/Apache) and could legally be
permissive, but we deliberately license it GPLv3 too. A future contributor will reasonably
ask "why isn't the brand-new host app MIT?" — the answer is that a single copyleft license
honors the project's lineage and the modding scene's norms, removes any ambiguity about
where the GPL boundary sits, and keeps host-app improvements open rather than absorbable
into closed forks.

## Considered Options

- **GPLv3 whole project** (chosen): one license, no boundary ambiguity, copyleft community fit.
- **Split (device GPLv3, host MIT/Apache)**: maximizes reuse of novel host code, but dual-license
  complexity, contributor confusion, and permits closed forks of the host.

## Consequences

- Host-app code cannot be reused in closed-source projects (acceptable; not a goal here).
- Relicensing later requires consent from all contributors — so this is fixed early on purpose.
- A `LICENSE` (GPLv3) file should be committed at the repo root before accepting contributions.
