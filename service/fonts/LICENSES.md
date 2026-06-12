# Vendored print fonts (AP-5 evidence export)

All faces are licensed under the SIL Open Font License 1.1 and were
decompressed (woff2 → ttf, no glyph or table edits beyond the container)
from the latin-subset woff2 files already vendored in
`console/src/fonts/` — see `console/src/fonts/LICENSES.md` for the
upstream provenance.

| File | Face | Upstream | License |
| --- | --- | --- | --- |
| Inter-Regular.ttf | Inter 400 (latin subset) | rsms/inter | OFL 1.1 |
| Inter-Bold.ttf | Inter 600 (latin subset; stands in for bold) | rsms/inter | OFL 1.1 |
| IBMPlexMono-Regular.ttf | IBM Plex Mono 400 (latin subset) | IBM/plex | OFL 1.1 |
| IBMPlexMono-Bold.ttf | IBM Plex Mono 500 (stands in for bold) | IBM/plex | OFL 1.1 |
| SourceSerif4-Regular.ttf | Source Serif 4 400 (latin subset) | adobe-fonts/source-serif | OFL 1.1 |
| SourceSerif4-Bold.ttf | Source Serif 4 600 (stands in for bold) | adobe-fonts/source-serif | OFL 1.1 |

The subsets vendor no italic faces; the PDF font families reuse the
uprights for italic slots (flagged in the AP-5 closeout).
