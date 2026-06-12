// The capture half of the pipeline: verify the REAL stack, then drive the
// console through the six beats with deliberate, human-paced, fully
// scripted interactions — recording per-beat webm takes and rendering the
// card PNGs from literal log bytes. A CLIENT only: HTTP, a browser, and
// the audit JSONL file. Never captures a broken or mislabeled stack.
//
// Usage:
//   node src/capture.ts --state-dir <dir> [--fresh-audit]
//     [--service http://127.0.0.1:8787] [--console http://127.0.0.1:3000]
//     [--config ../service/config.demo.json] [--out out] [--only B1]

import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
  statSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import type { Browser, BrowserContext, Locator, Page } from "playwright";

import beatsJson from "../beats.json" with { type: "json" };
import { cardHtml, loadConsoleFonts, renderCardPng } from "./cards.ts";
import type { CardFonts } from "./cards.ts";
import { cursorInitScript } from "./cursor-inject.ts";
import { extractFooterBlock, extractPdfText } from "./pdf-footer.ts";
import type { BeatFacts } from "./take-report.ts";

const here = dirname(fileURLToPath(import.meta.url));
const reelRoot = resolve(here, "..");
const repoRoot = resolve(reelRoot, "..");

const PACING = beatsJson.pacing;

interface Args {
  stateDir: string;
  freshAudit: boolean;
  service: string;
  console: string;
  config: string;
  out: string;
  only: string | null;
}

function parseArgs(argv: string[]): Args {
  const get = (flag: string): string | null => {
    const at = argv.indexOf(flag);
    return at >= 0 && at + 1 < argv.length ? argv[at + 1] : null;
  };
  const stateDir = get("--state-dir");
  if (!stateDir) {
    fail("--state-dir <dir> is required (where the service writes audit.jsonl)");
  }
  return {
    stateDir: resolve(stateDir!),
    freshAudit: argv.includes("--fresh-audit"),
    service: get("--service") ?? "http://127.0.0.1:8787",
    console: get("--console") ?? "http://127.0.0.1:3000",
    config: resolve(get("--config") ?? join(repoRoot, "service", "config.demo.json")),
    out: resolve(get("--out") ?? join(reelRoot, "out")),
    only: get("--only"),
  };
}

function fail(message: string): never {
  console.error(`\nREFUSED: ${message}\n`);
  process.exit(1);
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

// ---------------------------------------------------------------------------
// Prerequisites — never capture a broken stack
// ---------------------------------------------------------------------------

function checkFfmpeg(): void {
  const probe = spawnSync("ffmpeg", ["-version"], { encoding: "utf8" });
  if (probe.error || probe.status !== 0) {
    fail(
      "ffmpeg is not on PATH. Install it first (Windows: winget install ffmpeg) and re-run.",
    );
  }
}

async function checkService(serviceUrl: string): Promise<void> {
  try {
    const response = await fetch(`${serviceUrl}/healthz`);
    if (!response.ok) {
      fail(`service /healthz answered ${response.status} — start the stack first`);
    }
  } catch {
    fail(`service unreachable at ${serviceUrl} — start the stack first`);
  }
}

function checkDemoProfile(configPath: string): { embedModel: string; chatModel: string; endpoint: string } {
  if (!existsSync(configPath)) {
    fail(`service config not found at ${configPath} — pass --config <path to the demo config>`);
  }
  const config = JSON.parse(readFileSync(configPath, "utf8"));
  const profile: string = config.profile ?? "";
  if (!profile.toLowerCase().includes("demo")) {
    fail(
      `the config at ${configPath} does not carry the demo label (profile: ${JSON.stringify(profile)}). ` +
        "Refusing to film the production profile — the judge would never apply.",
    );
  }
  return {
    embedModel: config.embed_model ?? "nomic-embed-text",
    chatModel: config.judge_model ?? config.generate_model ?? "llama3.2:3b",
    endpoint: config.endpoint ?? "http://127.0.0.1:11434",
  };
}

async function checkConsole(consoleUrl: string): Promise<void> {
  try {
    const response = await fetch(consoleUrl);
    if (!response.ok) {
      fail(`console answered ${response.status} at ${consoleUrl}`);
    }
  } catch {
    fail(`console unreachable at ${consoleUrl} — npm run dev in /console first`);
  }
}

async function warmOllama(models: { embedModel: string; chatModel: string; endpoint: string }): Promise<void> {
  console.log(`warming ${models.embedModel} (embed)…`);
  try {
    const embed = await fetch(`${models.endpoint}/api/embed`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ model: models.embedModel, input: "warm-up" }),
      signal: AbortSignal.timeout(180_000),
    });
    if (!embed.ok) {
      fail(`Ollama embed warm-up failed (${embed.status}) for ${models.embedModel}`);
    }
  } catch (error) {
    fail(`Ollama embed warm-up failed: ${String(error)}`);
  }
  console.log(`warming ${models.chatModel} (chat)…`);
  try {
    const chat = await fetch(`${models.endpoint}/api/chat`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        model: models.chatModel,
        messages: [{ role: "user", content: "ok" }],
        stream: false,
        options: { num_predict: 1 },
      }),
      signal: AbortSignal.timeout(300_000),
    });
    if (!chat.ok) {
      fail(`Ollama chat warm-up failed (${chat.status}) for ${models.chatModel}`);
    }
  } catch (error) {
    fail(`Ollama chat warm-up failed: ${String(error)}`);
  }
}

function prepareAudit(stateDir: string, freshAudit: boolean): string {
  if (!existsSync(stateDir)) {
    fail(`--state-dir ${stateDir} does not exist (it must be the running service's state dir)`);
  }
  const auditPath = join(stateDir, "audit.jsonl");
  if (!existsSync(auditPath)) {
    writeFileSync(auditPath, "");
  } else if (freshAudit) {
    // Truncate ONLY when explicitly asked: a fresh shoot wants a fresh log.
    writeFileSync(auditPath, "");
    console.log("audit.jsonl truncated (--fresh-audit)");
  }
  return auditPath;
}

function auditTail(auditPath: string, predicate: (line: string) => boolean, count: number): string {
  const lines = readFileSync(auditPath, "utf8")
    .split("\n")
    .filter((l) => l.trim().length > 0 && predicate(l));
  if (lines.length < count) {
    fail(`audit log carries ${lines.length} matching rows; expected at least ${count}`);
  }
  return lines.slice(-count).join("\n");
}

// ---------------------------------------------------------------------------
// Deliberate, human-paced interaction helpers (the NUMBERS)
// ---------------------------------------------------------------------------

async function moveTo(page: Page, locator: Locator): Promise<void> {
  await locator.waitFor({ state: "visible" });
  const box = await locator.boundingBox();
  if (!box) {
    throw new Error("locator has no box to move to");
  }
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, {
    steps: PACING.mouseSteps,
  });
}

/** 80ms settle before every click; 600ms hold after every state change. */
async function settleClick(page: Page, locator: Locator): Promise<void> {
  await moveTo(page, locator);
  await sleep(PACING.clickSettleMs);
  await locator.click();
  await sleep(PACING.stateHoldMs);
}

async function typeInto(page: Page, locator: Locator, text: string): Promise<void> {
  await settleClick(page, locator);
  await locator.pressSequentially(text, { delay: PACING.keystrokeMs });
}

/** Smooth scroll at the contract's 400px/s (20px every 50ms). */
async function smoothScrollBy(page: Page, px: number): Promise<void> {
  const step = px > 0 ? 20 : -20;
  const ticks = Math.floor(Math.abs(px) / 20);
  for (let i = 0; i < ticks; i++) {
    await page.mouse.wheel(0, step);
    await sleep(50);
  }
}

async function smoothScrollUntilVisible(page: Page, locator: Locator, maxPx = 20_000): Promise<void> {
  let scrolled = 0;
  while (scrolled < maxPx) {
    const box = await locator.boundingBox().catch(() => null);
    if (box && box.y >= 0 && box.y + box.height <= 1000) {
      return;
    }
    await smoothScrollBy(page, 400);
    scrolled += 400;
  }
  await locator.scrollIntoViewIfNeeded();
}

// ---------------------------------------------------------------------------
// Takes
// ---------------------------------------------------------------------------

interface Take {
  context: BrowserContext;
  page: Page;
}

async function newTake(browser: Browser, rawDir: string): Promise<Take> {
  const context = await browser.newContext({
    viewport: { width: beatsJson.video.width, height: beatsJson.video.height },
    recordVideo: {
      dir: rawDir,
      size: { width: beatsJson.video.width, height: beatsJson.video.height },
    },
    acceptDownloads: true,
  });
  await context.addInitScript(cursorInitScript);
  const page = await context.newPage();
  return { context, page };
}

async function endTake(take: Take, outPath: string): Promise<void> {
  const video = take.page.video();
  await take.context.close();
  if (!video) {
    throw new Error("no video recorded for take");
  }
  const recorded = await video.path();
  copyFileSync(recorded, outPath);
}

// ---------------------------------------------------------------------------
// The beats
// ---------------------------------------------------------------------------

async function sceneAsk(page: Page, args: Args): Promise<BeatFacts> {
  await page.goto(`${args.console}/?as=p060`);
  await page.getByTestId("query-input").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  await typeInto(page, page.getByTestId("query-input"), "payroll salary review");
  await settleClick(page, page.getByTestId("toggle-hybrid"));
  await settleClick(page, page.getByTestId("toggle-judge"));

  const envelopePromise = page.waitForResponse(
    (response) => response.url().endsWith("/ask") && response.request().method() === "POST",
    { timeout: 240_000 },
  );
  await settleClick(page, page.getByTestId("ask-button"));
  const envelopeResponse = await envelopePromise;
  const envelope = (await envelopeResponse.json()) as {
    judge_applied: boolean;
    retrieval_mode: string;
    generation_applied: boolean;
  };
  await page.getByTestId("answer-card").waitFor({ state: "visible", timeout: 240_000 });
  await sleep(PACING.stateHoldMs);

  // Hover the provenance strip 2s.
  await moveTo(page, page.getByTestId("provenance-strip"));
  await sleep(2_000);

  // Click a citation chip; inspector open 3s.
  const chips = page.getByTestId("citation-chip");
  if ((await chips.count()) > 0) {
    await settleClick(page, chips.first());
    await page.getByTestId("doc-inspector").waitFor({ state: "visible" });
    await sleep(3_000);
  } else {
    // Honest degradation: no citations to click — hold the answer instead.
    await sleep(3_000);
  }

  const degradation: string[] = [];
  if (!envelope.judge_applied) {
    degradation.push("judge requested but NOT applied — record the elide line");
  }
  if (envelope.retrieval_mode !== "hybrid") {
    degradation.push(`retrieval degraded to ${envelope.retrieval_mode}`);
  }
  return {
    id: "B1",
    kind: "capture",
    scene: "ask",
    judge_requested: true,
    judge_applied: envelope.judge_applied,
    retrieval_mode: envelope.retrieval_mode,
    generation_applied: envelope.generation_applied,
    degradation: degradation.length > 0 ? degradation.join("; ") : undefined,
  };
}

async function sceneLens(page: Page, args: Args): Promise<BeatFacts> {
  await page.goto(`${args.console}/lens?as=p060`);
  await page.getByTestId("masthead").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  // Scroll the reason sections slowly, down then partly back.
  await smoothScrollBy(page, 1600);
  await sleep(PACING.stateHoldMs);
  await smoothScrollBy(page, -1600);
  await sleep(PACING.stateHoldMs);

  // Actor switch to p061 via the lens bar — the iris.
  await typeInto(page, page.getByTestId("principal-search"), "p061");
  await settleClick(page, page.getByTestId("principal-row").filter({ hasText: /^p061$/ }).first());
  await page.getByTestId("masthead").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  // Open p060's lens (cross-lens) and hold the audited line 2.5s.
  await typeInto(page, page.getByTestId("subject-search"), "p060");
  await settleClick(page, page.getByTestId("subject-row").filter({ hasText: /^p060$/ }).first());
  await page.getByTestId("cross-lens-line").waitFor({ state: "visible" });
  await moveTo(page, page.getByTestId("cross-lens-line"));
  await sleep(2_500);

  // Back to p061's own lens, where the agent emblem lives, and click it.
  // (Deviation flagged: p060 owns no agents, so the emblem click happens on
  // p061's self lens — one extra navigation step so the click is real.)
  await typeInto(page, page.getByTestId("subject-search"), "p061");
  await settleClick(page, page.getByTestId("subject-row").filter({ hasText: /^p061$/ }).first());
  await page.getByTestId("agent-emblem").first().waitFor({ state: "visible" });
  await settleClick(page, page.getByTestId("agent-emblem").first());
  await page.getByTestId("cross-lens-line").waitFor({ state: "visible" });
  await sleep(3_000);

  return {
    id: "B2",
    kind: "capture",
    scene: "lens",
    notes:
      "emblem click happens on p061's self lens (p060 owns no agents) — flagged deviation from the beat's literal order",
  };
}

async function sceneLane(page: Page, args: Args): Promise<BeatFacts> {
  await page.goto(`${args.console}/lane?as=p060`);
  await page.getByTestId("lane-box").first().waitFor({ state: "visible", timeout: 60_000 });
  await sleep(PACING.stateHoldMs);

  // Scroll the boxes slowly, down and partly back (the contract's 400px/s).
  await smoothScrollBy(page, 1200);
  await sleep(PACING.stateHoldMs);
  await smoothScrollBy(page, -1200);
  await sleep(PACING.stateHoldMs);

  // Explain the first card: the provenance path lights.
  const first = page.getByTestId("lane-box").first();
  await settleClick(page, first.getByTestId("box-explain"));

  // Follow the capability crumb through the AP-3 entry door into Atlas —
  // a real full-page navigation; the sheet opens on arrival.
  await settleClick(page, first.getByTestId("crumb-atlas"));
  await page
    .getByTestId("capability-sheet")
    .waitFor({ state: "visible", timeout: 60_000 });
  await sleep(4_000);

  return {
    id: "B2b",
    kind: "capture",
    scene: "lane",
    notes: "explain-this-box -> /atlas?cap=… entry door; the sheet opened on arrival",
  };
}

async function sceneDiffHr(page: Page, args: Args): Promise<BeatFacts> {
  await page.goto(`${args.console}/lens?as=p016`);
  await page.getByTestId("masthead").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  await typeInto(page, page.getByTestId("compare-search"), "p087");
  await settleClick(page, page.getByTestId("compare-row").filter({ hasText: /^p087$/ }).first());
  await page.getByTestId("diff-audited-line").waitFor({ state: "visible", timeout: 60_000 });
  await sleep(PACING.stateHoldMs);

  // The divergent HR row leads the shared table; scroll to d0093.
  const row = page.getByTestId("shared-row").filter({ hasText: "d0093" }).first();
  await smoothScrollUntilVisible(page, row);
  await moveTo(page, row);
  await sleep(4_000);

  return { id: "B3", kind: "capture", scene: "diff-hr" };
}

async function gotoDiffAgent(page: Page, args: Args): Promise<void> {
  await page.goto(`${args.console}/lens?as=p061`);
  await page.getByTestId("masthead").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  // Left = the room's subject: cross to p060 first.
  await typeInto(page, page.getByTestId("subject-search"), "p060");
  await settleClick(page, page.getByTestId("subject-row").filter({ hasText: /^p060$/ }).first());
  await page.getByTestId("cross-lens-line").waitFor({ state: "visible" });
  await sleep(PACING.stateHoldMs);

  await typeInto(page, page.getByTestId("compare-search"), "agent_finance");
  await settleClick(
    page,
    page.getByTestId("compare-row").filter({ hasText: /^agent_finance_analyst$/ }).first(),
  );
  await page.getByTestId("diff-audited-line").waitFor({ state: "visible", timeout: 60_000 });
  await sleep(PACING.stateHoldMs);
}

async function sceneDiffAgent(page: Page, args: Args): Promise<BeatFacts> {
  await gotoDiffAgent(page, args);
  const payroll = page
    .getByTestId("diff-section-rule")
    .filter({ hasText: "REBAC:grp_payroll_admins" })
    .first();
  await smoothScrollUntilVisible(page, payroll);
  await moveTo(page, payroll);
  await sleep(4_000);
  return { id: "B4", kind: "capture", scene: "diff-agent" };
}

async function sceneExport(
  page: Page,
  args: Args,
  fonts: CardFonts,
  outDir: string,
): Promise<BeatFacts> {
  await gotoDiffAgent(page, args);

  const downloadPromise = page.waitForEvent("download", { timeout: 120_000 });
  await settleClick(page, page.getByTestId("export-evidence"));
  const download = await downloadPromise;
  const pdfPath = join(outDir, "evidence-diff-p060-agent.pdf");
  await download.saveAs(pdfPath);
  await sleep(PACING.stateHoldMs);

  // Render the EXPORT-FOOTER card from the REAL PDF's extracted footer and
  // show it 5s inside this take.
  const footer = extractFooterBlock(extractPdfText(readFileSync(pdfPath)));
  writeFileSync(join(outDir, "cards", "B5-footer-source.txt"), footer);
  const html = cardHtml(
    { kind: "footer", heading: "The attestation footer — extracted from the downloaded PDF", raw: footer },
    fonts,
  );
  await page.setContent(html, { waitUntil: "load" });
  await sleep(5_000);

  return {
    id: "B5",
    kind: "capture",
    scene: "export",
    notes: `download saved to ${pdfPath}`,
  };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  checkFfmpeg();
  await checkService(args.service);
  const models = checkDemoProfile(args.config);
  await checkConsole(args.console);
  await warmOllama(models);
  const auditPath = prepareAudit(args.stateDir, args.freshAudit);

  const takesDir = join(args.out, "takes");
  const rawDir = join(args.out, "takes-raw");
  const cardsDir = join(args.out, "cards");
  for (const dir of [args.out, takesDir, rawDir, cardsDir]) {
    mkdirSync(dir, { recursive: true });
  }
  const fonts = loadConsoleFonts(repoRoot);

  const browser = await chromium.launch();
  const cardPage = await browser.newPage();
  const facts: BeatFacts[] = [];
  const wants = (id: string): boolean => args.only === null || args.only === id;

  const renderCard = async (id: string, htmlSource: { html: string; sourcePath?: string; raw?: string }) => {
    if (htmlSource.raw !== undefined && htmlSource.sourcePath) {
      writeFileSync(htmlSource.sourcePath, htmlSource.raw);
    }
    await renderCardPng(cardPage, htmlSource.html, join(cardsDir, `${id}.png`));
    console.log(`card ${id} rendered`);
  };

  for (const beat of beatsJson.beats) {
    if (!wants(beat.id)) {
      continue;
    }
    console.log(`— beat ${beat.id} (${beat.kind})`);
    if (beat.kind === "card") {
      switch (beat.card) {
        case "title":
          await renderCard(beat.id, { html: cardHtml({ kind: "title", text: beat.text! }, fonts) });
          facts.push({ id: beat.id, kind: "card" });
          break;
        case "numbers":
          await renderCard(beat.id, { html: cardHtml({ kind: "numbers", text: beat.text! }, fonts) });
          facts.push({ id: beat.id, kind: "card" });
          break;
        case "close":
          await renderCard(beat.id, { html: cardHtml({ kind: "close", text: beat.text! }, fonts) });
          facts.push({ id: beat.id, kind: "card" });
          break;
        case "audit-lens": {
          const raw = auditTail(auditPath, (l) => l.includes("\"lens_view\""), 1);
          await renderCard(beat.id, {
            html: cardHtml(
              { kind: "audit", heading: "The look is the governed event — the real audit row", raw },
              fonts,
            ),
            sourcePath: join(cardsDir, `${beat.id}-source.txt`),
            raw,
          });
          facts.push({ id: beat.id, kind: "card", notes: "audit row read from the live log" });
          break;
        }
        case "audit-export": {
          const raw = [
            auditTail(auditPath, (l) => l.includes("\"lens_diff\""), 1),
            auditTail(auditPath, (l) => l.includes("\"evidence_export\""), 1),
          ].join("\n");
          await renderCard(beat.id, {
            html: cardHtml(
              { kind: "audit", heading: "One act, two rows — the look and the export", raw },
              fonts,
            ),
            sourcePath: join(cardsDir, `${beat.id}-source.txt`),
            raw,
          });
          facts.push({ id: beat.id, kind: "card", notes: "audit rows read from the live log" });
          break;
        }
        default:
          fail(`unknown card kind ${beat.card} in beats.json`);
      }
      continue;
    }

    const take = await newTake(browser, rawDir);
    try {
      let fact: BeatFacts;
      switch (beat.scene) {
        case "ask":
          fact = await sceneAsk(take.page, args);
          break;
        case "lens":
          fact = await sceneLens(take.page, args);
          break;
        case "lane":
          fact = await sceneLane(take.page, args);
          break;
        case "diff-hr":
          fact = await sceneDiffHr(take.page, args);
          break;
        case "diff-agent":
          fact = await sceneDiffAgent(take.page, args);
          break;
        case "export":
          fact = await sceneExport(take.page, args, fonts, args.out);
          break;
        default:
          throw new Error(`unknown scene ${beat.scene} in beats.json`);
      }
      await endTake(take, join(takesDir, `${beat.id}.webm`));
      const size = statSync(join(takesDir, `${beat.id}.webm`)).size;
      console.log(`take ${beat.id} saved (${(size / 1_048_576).toFixed(1)} MiB)`);
      facts.push(fact);
    } catch (error) {
      await take.context.close().catch(() => {});
      await browser.close().catch(() => {});
      fail(`beat ${beat.id} failed: ${String(error)}`);
    }
  }

  await browser.close();
  writeFileSync(join(args.out, "take-report.json"), JSON.stringify(facts, null, 2));
  console.log(`\ncapture complete — facts in ${join(args.out, "take-report.json")}`);
}

await main();
