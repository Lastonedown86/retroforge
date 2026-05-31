# Mandatory NAND-backup gate before any persistent write

RetroForge blocks any persistent NAND install until the user has a full, verified
(size/hash-checked) NAND backup stored locally, and always exposes a one-click restore.
The backup is offered automatically as a one-time gate. A future contributor optimizing the
"connect → install" funnel will be tempted to make this skippable — this ADR records that the
friction is deliberate. A full NAND dump is the only true un-brick path; in the Hakchi
community, the users who skipped backups and bricked are the loudest cautionary tales, and a
guided-first tool must not let a novice walk into that.

## Considered Options

- **Mandatory gate** (chosen): one-time friction, maximal safety.
- **Strongly recommend but skippable**: less friction, but the one unbacked brick becomes a
  loud bad review.
- **Optional menu feature** (Hakchi's model): lowest friction, highest real-world brick rate
  — incompatible with a novice-first tool.
