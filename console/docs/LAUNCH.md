# Cold start — service (:8787) + console (:3000)

The exact commands used to bring the demo stack up from a cold machine,
in order, copy-pasteable for PowerShell from the repo root
(`C:\Users\princ\Documents\enterprise-brain`). These are the commands the
live-probe and evidence-capture session actually ran.

## 1) Build and start the engine on :8787

```powershell
cargo build --release -p service
.\target\release\service.exe --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx --agents-config config/agents.example.json --state-dir .state/agent-store
```

The service listens on `127.0.0.1:8787`. Check it from another shell:

```powershell
curl.exe -s -o NUL -w "%{http_code}" http://127.0.0.1:8787/healthz   # expect 200
```

## 2) Start the console on :3000 (another shell)

CORS allow-lists `:3000` — do not serve the console on any other port.
If the console was previously run from a different branch, clear the stale
Next.js dev bundle first (a stale bundle can carry retired auth headers).

```powershell
cd console
if (Test-Path .next) { Remove-Item -Recurse -Force .next }
npm install
npm run dev
```

Open <http://localhost:3000> — the front door is the identity picker.

## Stopping

```powershell
Get-NetTCPConnection -LocalPort 8787 -State Listen | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
Get-NetTCPConnection -LocalPort 3000 -State Listen | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force }
```

## Notes

- This build runs keyword-only retrieval: the two Ask toggles ("Broad
  search", "Verified answers") are disabled by design. Every answer still
  shows its sources.
- The walkthrough of what to click once it's up is in
  [FIRST_RUN.md](FIRST_RUN.md).
