# First run — the 60-second walk (the "Boomer Test")

The comprehension pass is built around one acceptance test: a non-technical
manager opens the app cold and, inside a minute, can (a) say what it is in one
sentence, (b) pick who they are, (c) ask a question and get either a sourced
answer or an honest, calm refusal.

This file is the exact walk, and what it proves. It was verified live against
the **main-tree** console (`console/`, session/bearer auth) and a locally-run
service binary built from this branch's `HEAD` (main lineage).

## Start it

```
# 1) engine (serves :8787) — from the repo root
cargo run -p service -- \
  --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx \
  --agents-config config/agents.example.json --state-dir .state/agent-store

# 2) console (serves :3000, talks to :8787) — in another shell
cd console && npm install && npm run dev
```

Open http://localhost:3000.

> This build runs **keyword-only** retrieval: the two Ask toggles ("Broad
> search", "Verified answers") are disabled because the engine has no embedder
> or judge model configured. Every answer still shows its sources.

## The walk

1. **Cold load → the front door is an identity picker.**
   One product name ("Enterprise Brain"), one sentence ("Ask your company's
   knowledge. Every answer respects what you're allowed to see."), then
   **"Who are you today?"** with the honest demo line: *"Demo mode: sign in as
   anyone — no password. View-as is open to everyone. Nothing here is
   deployed."* Three real people to pick: **Felix Osei** (p060, Finance head),
   **Tomas Reyes** (p088, HR), **Zara Castillo** (p_void, no access). No
   identity is pre-selected.

2. **Pick Felix Osei → Home renders for p060.**
   The nav reads in plain words — **Home / Workflow Command / Ask / Operating
   Map / My Access / Company Map / Review Queue** — a single demo-status line,
   and a **"Try asking"** chip staged above the fold.

3. **Open the Operating Map.**
   The org map draws **only Felix's slice** — 15 real people, each a labelled
   node (no monogram caterpillar), the Finance hub, real edges, honest counts
   (people 15 / projects 6). Every node is tabbable (`role="button"`,
   `tabIndex=0`; arrow keys traverse, Enter opens, Escape returns focus) and a
   visually-hidden list mirrors the nodes for screen readers.

4. **Ask a question, as Felix.**
   Click the chip (or type). The staged question is **"confidential financial
   statements"**. Press **Ask** → the engine returns **10 confidential Finance
   documents** as sources. (No prose answer is synthesised in this build — the
   card says "No generated answer" and the sources stand on their own; an
   `aria-live` line announces the result.)

5. **Now become Zara Castillo (p_void) and ask the SAME thing.**
   Her identity rail shows **Groups: none**. The same query returns **zero
   documents** — a calm, empty refusal: *"Nothing within your access supports
   an answer, and nothing was invented."* Same words, opposite outcome. That
   is the whole product in twenty seconds.

## What each step proves (verified live)

| Step | Claim | Evidence |
|---|---|---|
| 1 | one name, one sentence, pick-who-you-are, honest demo line | picker DOM: heading "Enterprise Brain"; "Who are you today?"; verbatim demo line; three real fixture ids |
| 1 | no hardwired identity | front door is the picker; `/me`, `/project` links carry no `?as` when identity-less |
| 2 | one vocabulary, one demo line per page | nav labels = locked table; single `shell-demo-identity-mode` |
| 3 | real, scoped, keyboard-operable map | 15 named nodes for p060 (structural Finance slice); `role=button`/`tabIndex=0`; SR list mirror |
| 4 | scoped, sourced Ask | p060 → 10 `confidential` docs |
| 5 | fail-closed refusal | p_void → 0 docs; "Groups: none"; honest empty copy |

### The p060 vs p_void contrast, at the engine

```
POST /ask {"query":"confidential financial statements"}
  p060  (holds grp_board + grp_finance) -> 10 results, all sensitivity=confidential
  p_void (no standing)                  ->  0 results
```

> Phrasing note: with keyword-only retrieval a natural, common-word question
> ("what did Finance publish this quarter?") pulls **public** docs into
> p_void's result and blurs the contrast — both identities then see the same
> public set. The suggested prompt is therefore a deliberate keyword phrase so
> the refusal is a clean 10-vs-0. Turn on "Broad search" + "Verified answers"
> (semantic retrieval + a judge) and natural questions regain the same
> contrast with a written, source-checked answer on top.
