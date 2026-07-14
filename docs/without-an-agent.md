# Using Enterprise Brain without an AI agent

**The language model is the optional layer.** The parts underneath it — the
enforcement engine, scoped retrieval, and the hash-chained audit trail — work
on their own. If you never point a model at Enterprise Brain, you still have a
**permission-aware search API** and a **tamper-evident record of every access**.

"Without an AI agent" here means *without an LLM in the loop*. The caller is
still an authenticated workload identity (a service, a search UI, a compliance
job) — it simply isn't a model, and no text ever gets synthesized.

## What you get with no model attached

The [`/v1` machine surface](reference/v1-api.md) is three endpoints, none of
which involve an LLM:

- **`POST /v1/retrieve`** — governed search. Candidates are scoped *at query
  construction* (EB-5): an out-of-scope document is never returned, not as an
  id, a title, a snippet, or a rank. This is the search primitive.
- **`GET /v1/documents/{id}`** — the full authorized body, or [THE
  404](reference/v1-api.md#the-deliberate-silences) — indistinguishable from
  "does not exist."
- **`GET /v1/whoami`** — who the caller resolves to.

Every one of those calls writes a hash-chained ledger row (EB-6). The answer
generation you see in the console's *Ask* room is a separate, optional layer
that sits on top of `retrieve` — turn it off (or never configure a model) and
everything above still holds.

## The same seam, no model in sight

Two callers, one confidential document, using only governed search + fetch:

```sh
# A caller cleared for the document
curl -sX POST localhost:8787/v1/retrieve \
  -H "Authorization: Bearer $CLEARED" -H 'content-type: application/json' \
  -d '{"query":"supplier audit findings","top_k":5}'
# -> {"principal":"…","candidates":[{"doc_id":"…","title":"…","rank":1}, …]}

curl -s localhost:8787/v1/documents/<doc_id_from_above> \
  -H "Authorization: Bearer $CLEARED"
# -> 200, the full body

# A caller who is not cleared, same document id
curl -s -o /dev/null -w "%{http_code}\n" localhost:8787/v1/documents/<same_id> \
  -H "Authorization: Bearer $NOT_CLEARED"
# -> 404 (THE 404 — did not decide by document; the access model did)
```

No prompt, no model, no synthesis — just scoped retrieval and authorized
fetch, each a ledgered decision. The Python SDK is a thin client over exactly
these calls if you'd rather not curl (`eb.retrieve(...)`, `eb.get_document(...)`).

## Where this fits

- **Permission-aware enterprise search** — a search box whose results are
  already filtered to the caller's authority, with no post-filter seam to leak
  through.
- **A compliance / access-audit layer** — every read (and every deny) is a
  hash-chained row; `verify-ledger` proves the chain intact. Point non-AI
  applications at `/v1` and get an evidence trail for free.
- **A governed document API** for any app, AI or not.

## Honest boundaries

- **Authentication is still required.** `/v1` is JWT-only (the Entra agent
  bridge). "No AI agent" is about the *model*, not about skipping identity —
  the caller authenticates as a workload identity like any other.
- **Retrieval today is lexical (BM25)**, scoped. A semantic/vector path exists
  behind config with an explicit degradation doctrine; it is not the default.
- **This is the current `/v1` surface used plainly** — not a separately
  packaged product. A first-class *governance-only mode* (permission-aware
  search with no LLM layer as a distinct offering) is one of the directions on
  the [roadmap](https://github.com/princeanozie25-web/enterprise-brain/issues/2)
  — 👍 it there if it's what you'd use.

See [concepts](concepts.md) for the two-surface model and the invariants, and
[the `/v1` reference](reference/v1-api.md) for exact request/response shapes.
