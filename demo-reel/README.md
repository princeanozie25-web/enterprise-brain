# demo-reel — the authentic-capture video pipeline

Produces the Enterprise Brain demo video from the REAL running stack.
Nothing is generated; everything on screen is product footage recorded by a
scripted browser, or literal log/PDF bytes rendered legibly on cards. When
the product changes, rerun the pipeline and the video remakes itself.

The pipeline is a CLIENT: it drives the console over HTTP/browser, reads
the audit JSONL from the service's `--state-dir`, and never imports or
links any workspace crate or console source.

## Prerequisites

- **ffmpeg + ffprobe on PATH.** The scripts check first and STOP if absent
  (Windows: `winget install ffmpeg`). They never install it themselves.
- **Node 24+** (the scripts are TypeScript run via Node's native type
  stripping — no build step).
- `npm install` in this directory, then `npx playwright install chromium`
  (chromium only).
- **The live stack** (the capture refuses to film anything less):
  - Ollama on `127.0.0.1:11434` with the demo config's embed + chat models
    pulled. Both get a warm-up call before any frame is recorded.
  - The service on `127.0.0.1:8787`, started with the **labeled demo
    profile** and an audit store, e.g. from the repo root:
    `target\release\service.exe --fixtures fixtures --artifacts compiler\artifacts
    --idx retrieval\idx --config service\config.demo.json
    --agents-config config\agents.example.json --state-dir <state-dir>`
    The capture reads the config file and REFUSES to film a profile whose
    label is not demo — the judge would never apply on production timeouts.
  - The console on `127.0.0.1:3000` (`npm run dev` in `/console`).

## Run order

1. Stack up (above). Hit `/`, `/lens`, `/atlas` once each if using `next
   dev`, so first-compile lag never lands in a take.
2. **Capture** (records per-beat webm takes + card PNGs, writes
   `out/take-report.json`):

   ```
   node src/capture.ts --state-dir <state-dir> [--fresh-audit]
   ```

   `--fresh-audit` truncates `<state-dir>/audit.jsonl` for the shoot; the
   script never truncates without it. `--only B3` reshoots one beat.
3. **Record the VO** from `out/take-report.md`'s facts (see the contract).
4. **Assemble**:

   ```
   node src/assemble.ts
   ```

   Emits per-beat normalized segments, then the master:
   `out/master-<yyyy-mm-dd>.mp4` (1080p30, H.264 high, CRF 18, AAC 192k),
   plus `out/take-report.md`.

`npm test` runs the offline T-REEL suite (T-R1..T-R4) — no browser, no
service, no ffmpeg execution (the plan is golden-filed, not run).

## The VO contract

- Files land in `vo/` named `vo-<beat id>.wav` (`vo-B0.wav` … `vo-B7.wav`,
  including the card and lane beats `vo-B2a.wav`, `vo-B2b.wav`,
  `vo-B5a.wav`), one per beat, recorded off the video script **against the
  take-report's facts** — the
  report says per beat whether the judge APPLIED or was ELIDED, the
  retrieval mode, and any degradation, so the recorded line matches what
  actually happened on screen.
- Beat duration = `max(captured footage length, VO length + 0.5s)`.
  Footage shorter than the VO freeze-frames its last frame; footage is
  never cut from the head, and under this formula real footage is never
  lost (the explicit `-t` encode is head-anchored, so any overage falls off
  the tail).
- Missing or partial `vo/`: the beat assembles with generated silence of
  the script's target duration and a `VO PENDING` watermark bottom-left in
  ink-soft — the pipeline proves end-to-end before the voice exists.
- VO audio is mono → stereo, loudness-normalized to −16 LUFS; 0.4s of
  breathing room rides each beat's tail (the last beat excepted).

## Honesty notes

- The cursor on screen is an injected 14px ink ring tracking real pointer
  events — Playwright recordings don't capture the OS cursor. Faithful
  rendering of real interactions, not decoration.
- AUDIT and EXPORT-FOOTER cards render the literal bytes read at capture
  time (`out/cards/*-source.txt` keeps the source next to each PNG); a
  T-REEL test holds card text == source bytes. Cards never paraphrase.
- The B5 footer comes from the downloaded PDF via a minimal in-repo
  extractor scoped to the AP-5 export shape (`src/pdf-footer.ts`).
