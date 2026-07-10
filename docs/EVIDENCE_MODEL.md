# Evidence Model

TrailTransfer turns each finished job into a **tamper-evident, independently
verifiable record**. This is what makes transfers *audit-ready* rather than just
logged. It needs no database, no cloud, and no TrailMQ.

## The chain, in one picture

```
command (untrusted) ─▶ policy decision ─▶ rclone run ─▶ result payload
                                                              │  seal
                                                              ▼
   record N-1 ──chain_hash──▶ prev_hash │ record N │ chain_hash ──▶ prev_hash │ record N+1
                             (seq n-1)              (seq n)                    (seq n+1)
                         append-only JSONL journal (fsync)  ──▶  trailtransfer verify
```

Each result is wrapped in an **envelope** and linked to the previous one, so
removing, altering or reordering any record breaks the chain and is detected by
`verify`.

## The envelope

```json
{
  "schema_version": "1",
  "worker_id": "worker-01",
  "seq": 7,
  "prev_hash": "sha256:…",          // chain_hash of the previous record
  "event_type": "result",
  "recorded_at": "2026-01-01T10:00:12.5Z",
  "time_source": "system",
  "issuer": { "auth": "self-declared", "identity": "alice" },
  "payload": { /* the result event: action, source, target, status,
                  counters, command_hash, policy_version, policy_hash, … */ },
  "content_hash": "sha256:…",       // SHA-256 of JCS(payload)
  "chain_hash": "sha256:…",         // SHA-256 of JCS(envelope core incl. prev_hash)
  "signature": null                  // optional, Ed25519 (v0.3)
}
```

- **Canonicalisation:** [RFC 8785 JSON Canonicalization Scheme (JCS)](https://www.rfc-editor.org/rfc/rfc8785)
  — deterministic bytes regardless of key order, so hashes are stable and
  reproducible by anyone.
- **content_hash** = `SHA-256(JCS(payload))` — proves the job record was not
  altered.
- **chain_hash** = `SHA-256(JCS(core))` over `{schema_version, worker_id, seq,
  prev_hash, event_type, recorded_at, time_source, issuer, content_hash}` — links
  each record to its predecessor (chained hashing, in the spirit of
  [RFC 6962](https://www.rfc-editor.org/rfc/rfc6962)).
- **seq** is monotonic per worker; a gap is a missing record.
- **prev_hash** of the first record is the all-zero **genesis** hash.

## The journal

Set `TRAILTRANSFER_EVIDENCE_JOURNAL=/data/evidence/journal.jsonl` (a writable,
ideally WORM, volume). Each sealed record is appended as one JSON line and
`fsync`'d. On startup the chain state is recovered from the last record, so the
chain survives restarts. Without a journal the chain still links records within
one process lifetime but resets on restart — enable the journal for durable
("enduring") evidence.

## Verification

```bash
trailtransfer verify /data/evidence/journal.jsonl
# OK:  N record(s), hash chain intact              (exit 0)
# FAIL: content_hash mismatch / prev_hash break …  (exit 1)
```

`verify` recomputes every hash and checks the links. An auditor runs it against
the file alone.

## Attribution & time (honest limits)

- **issuer** is *self-declared* via the command's `operator` field. MQTT 3.1.1
  does not forward the publisher's authenticated identity to subscribers, so
  strong attribution is a **broker/governance** responsibility — enforce it with
  broker ACLs / mTLS / MQTT 5 user-properties (or TrailMQ).
- **time_source** is `system`. For regulated use, keep the host on **NTP**
  (ISO/IEC 27001:2022 control 8.17); optional **RFC 3161** trusted timestamps are
  planned (v0.3).

## Threat coverage & what closes the gaps

| Threat | Detected by v0.2? | Note |
|---|---|---|
| A record altered | ✅ | `content_hash` / `chain_hash` mismatch |
| A record deleted / reordered | ✅ | `prev_hash` break, `seq` gap |
| Whole-suffix rewrite by a journal-writer | ⚠️ not alone | close with **signatures** (v0.3) and/or **anchoring** the latest `chain_hash` to WORM / a TSA / TrailMQ |
| Payload data integrity (which files) | partial | add optional **file manifest** (path, size, sha256) — v0.3 |
| Non-repudiation of origin | ⚠️ | **Ed25519** envelope signature — v0.3 |

## Roadmap

- **v0.2 (this):** JCS, envelope, `seq`/`prev_hash` chain, journal, `verify`, issuer.
- **v0.3:** Ed25519 signatures, RFC 3161 timestamps, file manifest, WORM export helper.
