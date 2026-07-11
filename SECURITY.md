# Security policy

## Reporting a vulnerability

**Please report privately — never in a public issue.**

- Email: **princeanozie03@gmail.com** with subject `[SECURITY] enterprise-brain`.
- When this repository is public, GitHub **Private Vulnerability Reporting**
  will be enabled and preferred; until then, email is the channel.

Include what you can: the affected surface (`/v1`, console, bridge, ledger,
estate), a reproduction (curl or SDK snippet), the ledger rows or doctor
output you observed, and version/commit. **Never include real tokens or key
material** — redact them; a `kid` and claim *names* are enough.

## What counts

Anything that violates an invariant is a vulnerability by definition — above
all a **false allow** (content served to a principal the oracle would deny),
authority smuggled through a connector, an unledgered decision, a deny reason
leaking on the wire, or ledger tampering that verifies CLEAN.

## Response norms

Solo maintainer: acknowledgement within **72 hours**, a triage verdict within
**7 days**, best effort thereafter with priority over all other work.
**No bug bounty exists yet** — reports earn credit in the fix's release notes
(opt-out respected), stated plainly rather than implied otherwise.
