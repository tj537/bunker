#!/usr/bin/env bash
# Writes THIRD-PARTY-LICENSES.txt: the licenses of every crate that ships in the
# app (Mac and Windows targets, see about.toml), then the bundled fonts and icons.
# Run it after adding or updating a dependency. Needs: cargo install cargo-about --features cli
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=THIRD-PARTY-LICENSES.txt
cargo about generate --locked packaging/licenses/about.hbs -o "$OUT"

section() {
    printf '\n------------------------------------------------------------------------------\n%s\n------------------------------------------------------------------------------\n\n' "$1" >> "$OUT"
    cat "$2" >> "$OUT"
}
printf '\n\nFonts and icons bundled with Solarum\n====================================\n' >> "$OUT"
section "Space Grotesk (assets/fonts) – SIL Open Font License 1.1" assets/fonts/OFL-SpaceGrotesk.txt
section "Anton (assets/fonts) – SIL Open Font License 1.1" assets/fonts/OFL-Anton.txt
section "Phosphor Icons (via egui-phosphor) – MIT" packaging/licenses/Phosphor-Icons-MIT.txt
echo "==> Wrote $OUT"
