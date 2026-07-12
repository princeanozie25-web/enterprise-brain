# How to: add a source (the estate seam)

A second source joins the estate as **bytes plus a separate access model** —
never as bytes that carry their own permissions.

## The shape on disk

```text
<estate_dir>/
  s3-access.json           # THE authority: tiers, agent grants, object labels, content hash
  s3-store/<bucket>/<key>  # the objects: bytes only
```

`s3-access.json`:

```json
{
  "content_sha256": "<sha256 over doc_id\\0body\\0 in doc-id order>",
  "tier_levels": { "public": 0, "internal": 1, "confidential": 2 },
  "agent_tiers": { "agent_estate_confidential": "confidential" },
  "objects": [
    { "doc_id": "s3/<bucket>/<key>", "sensitivity": "internal" }
  ]
}
```

Every object on disk must have a label; every label must be a known tier;
the pinned `content_sha256` must match the ingested bytes — any mismatch
fails startup (a tampered store never serves). Principals absent from
`agent_tiers` are denied every object: the fail-closed seam default.

## Wire it

```json
{ "estate_dir": "fixtures/estate" }
```

in the ServiceConfig (the fixtures ship a 150-object demo estate at that
path). Restart; `service doctor` must show `estate` ✓ ("N objects verify
against the pinned hash; retrieval index builds").

## Writing a NEW connector

Filesystem-shaped sources reuse `FsBucketConnector`. Anything else implements
the `SourceConnector` trait (ingest-time only: `enumerate()` runs at startup,
never on the request path) and must pass the
[conformance kit](../connector-certification.md) — including the poisoned-
metadata probe: a connector that emits `acl`, `permissions`, `roles`, or any
permission-shaped key in `native_meta` is refused whole at ingest.
