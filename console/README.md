# Ask Brain Console (M3b)

The human face of the governed pipeline. The governance metadata IS the
product: scope, provenance, and honest degradation are legible at a glance —
a calm enterprise instrument panel, not a chatbot.

```sh
npm install
npm run dev     # :3000, against the live service on 127.0.0.1:8787
npm test        # U-1..U-5 (vitest + RTL, fully offline, fixture envelopes)
npm run build   # static export (out/) — type-checks the contract mirror
```

- **The contract:** `src/lib/api.ts` mirrors the service envelope and `/doc`
  types field-for-field. No extra fields, no convenience counts — the types
  cannot represent a count of suppressed documents (U-3 proves it at compile
  time and at runtime).
- **Identity rail:** the permanent "DEMO IDENTITY MODE" banner, the
  searchable virtualized principal switcher (124 demo ids, display-only —
  the service enforces scope regardless), and the always-visible
  "What I can see" panel.
- **Provenance strip:** four factual badges (retrieval mode, judge,
  generation, and — only when true — "aggregation rule applied"). Neutral
  tones; degradation is honesty, never styled as failure.
- **Quiet states:** empty results say "Nothing in your scope matches" (no
  counts, no hints); a null answer is "No generated answer"; every `/doc`
  404 renders one indistinguishable empty state (U-5).
- **Offline by construction:** system font stack, no CDN, no analytics, no
  telemetry (`NEXT_TELEMETRY_DISABLED=1` for builds); fixtures in
  `tests/fixtures/` are captured live envelopes (ids and numbers only),
  embedded as typed literals so `tsc` checks the mirror against reality.

All magic numbers live in `src/lib/constants.ts`.
