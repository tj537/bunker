## What does this change?

<!-- In a sentence or two: what changes for someone playing Solarum? Link the issue, e.g. "Fixes #12". -->

## How did you check it?

<!-- Screenshots (cargo test --release render_ -- --ignored) or a short WAV help a lot for UI and sound changes. -->

## Checklist

- [ ] `cargo fmt`
- [ ] `cargo clippy --release --all-targets -- -D warnings` has no warnings
- [ ] `cargo test --release` passes
- [ ] README.md / docs/ARCHITECTURE.md updated, if a feature changed
- [ ] New `Song` / `Track` / `Step` fields have `#[serde(default)]`
- [ ] New dependencies have an MIT-compatible license and THIRD-PARTY-LICENSES.txt is regenerated
