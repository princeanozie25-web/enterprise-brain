/**
 * SHOWREEL TRACK C — the 10-beat capture.
 *
 * Drives the FULL Felix journey headed at 1920x1080 (deviceScaleFactor 2,
 * dark theme) and produces BOTH a numbered still per beat AND one continuous
 * video of the whole run. The video is the Higgsfield source.
 *
 * Re-runnable by construction:
 *  - restarts the service on :8787 at the top of the run (fresh AUTH-4
 *    session store — the documented 64-session capture gotcha) on a FRESH
 *    showreel state dir, so every run starts from the same world;
 *  - starts the console dev server on :3000 if it is not already up;
 *  - identity switches are plain `?as=` navigations against the fresh
 *    session store (4 logins per run, nowhere near the 64 cap);
 *  - ends with a smoke assertion that every expected artifact exists and is
 *    non-empty, and prints the video path + duration.
 *
 * Run from the repo root (playwright is reused from demo-reel — no new dep):
 *   node console/docs/showreel/capture.mjs
 *
 * Prerequisites: `cargo build --release -p service` and Ollama serving
 * llama3.2:3b on :11434 (the live generator — beat 8 drafts for real).
 */
import { spawn, execSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, statSync, readdirSync, renameSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..", "..", "..");
const OUT = HERE;
const STILLS = join(OUT, "stills");
const VIDEO_DIR = join(OUT, "video");
const STATE_DIR = join(ROOT, ".state", "showreel-state");
const SERVICE = "http://127.0.0.1:8787";
const CONSOLE = "http://localhost:3000";

// The cast and the script (Phase 0): Felix proposes on a capability he
// actually leads; Ingrid (his manager, the CEO) is the accountable approver.
const FELIX = "p060";
const INGRID = "p113";
const VOID = "p_void";
const CAP = "cap03"; // Capability: Pick Accuracy 03 — Felix is Lead.
const ASK_QUERY = "confidential financial statements";
const PLAN_TITLE = "Onboarding new hires";
const PLAN_GOAL = "confidential financial statements";

const { chromium } = await import(
  pathToFileURL(join(ROOT, "demo-reel", "node_modules", "playwright", "index.mjs")).href
);

// --- helpers ---------------------------------------------------------------

async function waitHttp(url, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    try {
      const response = await fetch(url);
      if (response.status < 500) return;
    } catch {
      /* not up yet */
    }
    if (Date.now() > deadline) throw new Error(`${label} did not come up at ${url}`);
    await new Promise((r) => setTimeout(r, 500));
  }
}

function restartService() {
  try {
    execSync("taskkill /IM service.exe /F", { stdio: "ignore" });
  } catch {
    /* not running */
  }
  rmSync(STATE_DIR, { recursive: true, force: true });
  mkdirSync(STATE_DIR, { recursive: true });
  const child = spawn(
    join(ROOT, "target", "release", "service.exe"),
    [
      "--fixtures", "fixtures",
      "--artifacts", "compiler/artifacts",
      "--idx", "retrieval/idx",
      "--agents-config", "config/agents.example.json",
      "--state-dir", STATE_DIR,
      "--config", "service/config.json",
    ],
    { cwd: ROOT, stdio: "ignore", detached: true },
  );
  child.unref();
  return waitHttp(`${SERVICE}/healthz`, 30_000, "service");
}

async function ensureConsole() {
  try {
    const response = await fetch(CONSOLE);
    if (response.status < 500) return;
  } catch {
    /* not up */
  }
  const child = spawn("npm", ["run", "dev"], {
    cwd: join(ROOT, "console"),
    stdio: "ignore",
    detached: true,
    shell: true,
  });
  child.unref();
  await waitHttp(CONSOLE, 120_000, "console dev server");
}

/** Type into an input and VERIFY the value stuck. The shell remounts its
 * room subtree when the session resolves (#main is keyed by the active
 * principal); keystrokes racing that remount land on a detached node and
 * vanish. Once the value verifies, it lives in React state and survives any
 * later remount. */
async function typeInto(page, selector, text, delay) {
  const input = page.locator(selector);
  for (let attempt = 0; attempt < 5; attempt++) {
    await input.pressSequentially(text, { delay });
    if ((await input.inputValue()) === text) return;
    await input.fill("");
    await page.waitForTimeout(600); // let the remount settle, then retype
  }
  throw new Error(`typed text did not stick in ${selector}`);
}

/** Navigate to a routed surface and WAIT until the shell has actually bound
 * the expected identity (the LensBar chip shows it). A navigation that lost
 * its `?as` or whose login raced the room mount is retried once with a fresh
 * goto — logged, never silent. */
async function gotoAs(page, path, principal) {
  const url = `${CONSOLE}${path}${path.includes("?") ? "&" : "?"}as=${principal}`;
  for (let attempt = 0; attempt < 3; attempt++) {
    if (attempt > 0) console.log(`  (retrying ${path} as ${principal} — identity did not bind)`);
    await page.goto(url);
    try {
      await page.waitForFunction(
        (expected) =>
          document.querySelector('[data-testid="lens-current"]')?.textContent?.includes(expected),
        principal,
        { timeout: 8_000 },
      );
      return;
    } catch {
      /* retry */
    }
  }
  throw new Error(`identity ${principal} never bound on ${path}`);
}

let beatIndex = 0;
async function still(page, name) {
  beatIndex += 1;
  const file = join(STILLS, `${String(beatIndex).padStart(2, "0")}-${name}.png`);
  await page.waitForTimeout(900); // let the frame settle for the film
  await page.screenshot({ path: file });
  console.log(`  beat ${beatIndex}: ${name}`);
}

// --- the run ---------------------------------------------------------------

console.log("showreel: restarting service (fresh session store + state)…");
await restartService();
console.log("showreel: ensuring console dev server…");
await ensureConsole();
// Warm the generator so beat 8 drafts inside its 20s budget.
await fetch("http://127.0.0.1:11434/api/generate", {
  method: "POST",
  body: JSON.stringify({ model: "llama3.2:3b", prompt: "ok", stream: false, keep_alive: "30m" }),
}).catch(() => {});
// Warm the FULL ask pipeline (embedder load + retrieval + generation +
// grounding) with the exact beat-4/5 asks. The console's client budget for
// POST /ask is 20s; a cold first ask on a freshly restarted service can
// exceed it and the beat would show an honest timeout instead of the
// answer. Warming runs the same governed pipeline — nothing is staged.
async function warmAsk(principal) {
  const login = await fetch(`${SERVICE}/auth/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ principal_id: principal }),
  });
  const { session_token } = await login.json();
  await fetch(`${SERVICE}/ask`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${session_token}` },
    body: JSON.stringify({ query: ASK_QUERY }),
  }).catch(() => {});
}
console.log("showreel: warming the ask pipeline…");
await warmAsk(FELIX);
await warmAsk(VOID);

rmSync(STILLS, { recursive: true, force: true });
rmSync(VIDEO_DIR, { recursive: true, force: true });
mkdirSync(STILLS, { recursive: true });
mkdirSync(VIDEO_DIR, { recursive: true });

const browser = await chromium.launch({ headless: false });
const context = await browser.newContext({
  viewport: { width: 1920, height: 1080 },
  deviceScaleFactor: 2,
  recordVideo: { dir: VIDEO_DIR, size: { width: 1920, height: 1080 } },
});
const page = await context.newPage();
const pageErrors = [];
page.on("console", (m) => {
  if (m.type() === "error") pageErrors.push(m.text().slice(0, 200));
});
page.on("pageerror", (e) => pageErrors.push(`PAGEERROR ${String(e).slice(0, 200)}`));
const startedAt = Date.now();

try {
  // Beat 1 — the cinematic entry (cold open).
  await page.goto(`${CONSOLE}/`);
  await page.waitForSelector('[data-testid="entry-screen"]');
  await still(page, "entry");

  // Beat 2 — Enter the demo -> the reframed picker (the ONE transition).
  await page.click('[data-testid="entry-cta"]');
  await page.waitForSelector('[data-testid="identity-picker"]');
  await still(page, "picker");

  // Beat 3 — Felix's Home, scope masthead populated. The card is a plain
  // anchor to /me?as=p060; a rare click-vs-rerender race can land the shell
  // without the query, so verify the landed URL and, if the race hit, drive
  // to the anchor's own declared destination (identical navigation, logged).
  await page.click(`[data-testid="identity-option-${FELIX}"]`);
  await page.waitForURL("**/me*", { timeout: 15_000 });
  if (!page.url().includes(`as=${FELIX}`)) {
    console.log("  beat 3: click race detected — re-driving the card's declared href");
    await page.goto(`${CONSOLE}/me?as=${FELIX}`);
  }
  await page.waitForSelector('[data-testid="employee-dashboard"]', { timeout: 30_000 });
  await still(page, "home");

  // Beat 4 — the grounded, cited answer (K1). Type only after the session
  // is established (the scope chips populate then) — the login effect resets
  // room state when it resolves, which would wipe text typed too early.
  await gotoAs(page, "/ask", FELIX);
  await page.waitForSelector('[data-testid="scope-chip"]', { timeout: 15_000 });
  await typeInto(page, '[data-testid="query-input"]', ASK_QUERY, 30);
  await page.click('[data-testid="ask-button"]');
  await page.waitForSelector('[data-testid="answer-card"]', { timeout: 30_000 });
  await page.waitForSelector('[data-testid="citation-chip"]', { timeout: 5_000 });
  await still(page, "ask-answer");

  // Beat 5 — the same words from p_void: an honest refusal. p_void holds no
  // scope, so the session signal is the rail's honest empty state.
  await gotoAs(page, "/ask", VOID);
  await page.waitForSelector('[data-testid="scope-chip"], [data-testid="rail-empty-state"]', {
    timeout: 15_000,
  });
  await typeInto(page, '[data-testid="query-input"]', ASK_QUERY, 30);
  await page.click('[data-testid="ask-button"]');
  await page.waitForSelector('[data-testid="answer-card"], [data-testid="ask-error"]', {
    timeout: 30_000,
  });
  await still(page, "ask-refusal");

  // Beat 6 — Felix's live Operating Map (one clean frame). The room sits
  // behind the AdminPreviewGate: reveal it, then wait for the audited panel
  // and network idle (the documented capture gotcha) so the map is fully
  // drawn before the frame.
  await gotoAs(page, "/admin/graph", FELIX);
  await page.waitForSelector('[data-testid="admin-preview-gate-reveal"]', { timeout: 20_000 });
  await page.click('[data-testid="admin-preview-gate-reveal"]');
  await page.waitForSelector('[data-testid="graph-audit-panel"]', { timeout: 30_000 });
  await page.waitForLoadState("networkidle");
  await still(page, "map");

  // Beat 7 — the create flow, typed on camera.
  await gotoAs(page, `/project?cap=${CAP}`, FELIX);
  await page.waitForSelector('[data-testid="proposal-create-title"]', { timeout: 20_000 });
  await typeInto(page, '[data-testid="proposal-create-title"]', PLAN_TITLE, 45);
  await typeInto(page, '[data-testid="proposal-create-goal"]', PLAN_GOAL, 30);
  await still(page, "project-start");

  // Beat 8 — the grounded proposal, drafted LIVE (the climax). Bring the
  // card's boxes + anchor chips into frame before the still.
  await page.click('[data-testid="proposal-create-submit"]');
  await page.waitForSelector('[data-testid="proposal-card"]', { timeout: 45_000 });
  await page.locator('[data-testid="proposal-card"]').scrollIntoViewIfNeeded();
  await still(page, "proposal");

  // Beat 9 — the approver's Human Gate (Ingrid's side of the cut).
  await gotoAs(page, `/project?cap=${CAP}`, INGRID);
  await page.waitForSelector('[data-testid="proposal-card"][data-can-decide="true"]', {
    timeout: 20_000,
  });
  // The gate's live decision buttons must be in frame — this IS the beat.
  await page.locator('[data-testid="proposal-gate-approve"]').scrollIntoViewIfNeeded();
  await still(page, "gate");

  // Beat 10 — approve, then the payoff: real work in Felix's Next column.
  // The approver's list is a PENDING-ONLY inbox, so a recorded approval can
  // either show the feedback line or (on the refetch) retire the card from
  // the inbox entirely — both are the decision landing.
  await page.click('[data-testid="proposal-gate-approve"]');
  await page.waitForSelector(
    '[data-testid="proposal-gate-feedback"], [data-testid="proposals-empty"]',
    { timeout: 20_000 },
  );
  await page.waitForTimeout(800); // hold the decided frame for the film
  await gotoAs(page, `/project?cap=${CAP}`, FELIX);
  await page.waitForSelector('[data-testid="pipeline-board"]', { timeout: 20_000 });
  await page.waitForSelector(
    '[data-testid="pipeline-column"][data-stage="next"] [data-testid="pipeline-card"]',
    { timeout: 20_000 },
  );
  await still(page, "materialized");
} catch (error) {
  // A failing beat leaves a diagnostic frame + page state beside the stills.
  console.log(`FAILURE at url=${page.url()}`);
  console.log(`page console errors: ${JSON.stringify(pageErrors.slice(-6))}`);
  await page.screenshot({ path: join(OUT, "capture-failure.png") }).catch(() => {});
  throw error;
} finally {
  await context.close(); // flushes the video
  await browser.close();
}

// --- smoke assertions --------------------------------------------------------

const expected = [
  "01-entry.png", "02-picker.png", "03-home.png", "04-ask-answer.png",
  "05-ask-refusal.png", "06-map.png", "07-project-start.png",
  "08-proposal.png", "09-gate.png", "10-materialized.png",
];
const missing = expected.filter(
  (f) => !existsSync(join(STILLS, f)) || statSync(join(STILLS, f)).size === 0,
);
if (missing.length > 0) throw new Error(`missing/empty stills: ${missing.join(", ")}`);

const webms = readdirSync(VIDEO_DIR).filter((f) => f.endsWith(".webm"));
if (webms.length !== 1 || statSync(join(VIDEO_DIR, webms[0])).size === 0) {
  throw new Error(`expected exactly one non-empty video, saw: ${webms.join(", ")}`);
}
const finalVideo = join(VIDEO_DIR, "showreel-journey.webm");
renameSync(join(VIDEO_DIR, webms[0]), finalVideo);

console.log(`showreel: 10/10 stills in ${STILLS}`);
console.log(`showreel: video ${finalVideo} (${(statSync(finalVideo).size / 1e6).toFixed(1)} MB, ~${Math.round((Date.now() - startedAt) / 1000)}s run)`);
console.log("showreel: service left running on :8787 (stop: taskkill /IM service.exe /F)");
