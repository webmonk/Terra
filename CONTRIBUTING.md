# Contributing to Terra

Thanks for helping improve Terra. This document covers the basics for code contributions.

## Development setup

```bash
cargo build -p terra-app
cargo test --workspace
cargo run -p terra-app
```

Use a recent stable Rust toolchain. GPU features need a working wgpu backend (DX12 on Windows by default).

## Crate rules

| Crate | Must not depend on |
|-------|--------------------|
| `terra-core` | `wgpu`, any UI crate |
| `terra-gui` | `terra-core` or other domain crates |
| `terra-ui` | owning eval/GPU; prefer emitting `PanelAction` |

Keep domain content (layer kinds, presets, archetypes) in `terra-core` when practical. UI crates present and apply; they should not become a second catalog of truth.

## Pull request guidelines

- Prefer small, reviewable PRs (one concern: split a module, fix a bug, add a feature).
- Do not mix large refactors with feature work.
- Prefer modules under ~800–1000 lines; treat files over ~1500 lines as split candidates unless they are pure static data.
- Avoid decorative banner comments; document invariants briefly at module level.
- Prefer `Result` at fallible boundaries; reserve `expect` for true invariants with a clear reason.
- When adding a layer type, update the layer registry (and inspector family if needed) rather than hand-syncing multiple catalogs.

## Docs

- Public design docs live under `docs/architecture/` and `docs/algorithms/`.
- Do not add new `*_sprint.md` pass notes to the default docs tree; use issues/PRs or `docs/archive/` for historical notes.

## License

By contributing, you agree that your contributions are dual-licensed under MIT and Apache-2.0, as described in [LICENSE](LICENSE).
