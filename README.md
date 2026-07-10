# TrailTransfer

**MQTT-controlled rclone worker for audit-ready edge file transfers.**

TrailTransfer lets edge machines execute file-transfer jobs without exposing an
HTTP API, opening SSH, or relying on invisible cron scripts. Jobs arrive over
**MQTT**, are validated against a local **policy**, executed through **rclone**,
and reported back as structured **status / progress / result / evidence** events.

TrailTransfer does **not** replace rclone — rclone stays the transfer engine.
TrailTransfer is the MQTT control plane, policy layer, job manager and evidence
wrapper around it.

> Works with any MQTT broker. Best with TrailMQ.

```
MQTT command in
  → policy validation
  → controlled rclone execution (argv, never a shell)
  → progress / status / result out
  → sealed, hash-chained evidence record  →  trailtransfer verify
```

Every finished job (success **or** rejection) is sealed into a tamper-evident,
append-only journal and can be **independently verified** — no database, no
cloud, no TrailMQ. This is what makes it *audit-ready* and a building block for
ALCOA++/GxP-style data integrity (see [`docs/EVIDENCE_MODEL.md`](docs/EVIDENCE_MODEL.md)
and [`docs/COMPLIANCE.md`](docs/COMPLIANCE.md)).

## What it is / is not

**It is:** an MQTT-controlled edge transfer worker · a policy-guarded rclone
runner · an audit-ready job executor · a TrailMQ-native worker.

**It is not:** a replacement for rclone · a remote shell · an unrestricted file
manager · an enterprise MFT suite · a cloud service.

## Quickstart

```bash
docker compose up --build
```

In another terminal, watch everything the worker publishes, then send a command:

```bash
mosquitto_sub -h localhost -p 1883 -v -t 'trailtransfer/worker-01/#'

mosquitto_pub  -h localhost -p 1883 \
  -t trailtransfer/worker-01/commands \
  -f examples/commands/check.json
```

You'll see `accepted → started → … → completed` on the job's topics, ending in a
sealed **result envelope** (`seq`, `prev_hash`, `content_hash`, `chain_hash`)
whose `payload` carries `command_hash`, `policy_version`, `policy_hash` and
`worker_version`.

> `examples/commands/copy.json` targets MinIO (bucket created by the compose
> `createbucket` service). `examples/commands/sync-dry-run.json` is intentionally
> **rejected** by the demo policy — `sync` can delete on the target, so it is
> gated behind `allow_sync_delete` (default `false`) — to show what a policy
> denial looks like.

## Example command

```json
{
  "job_id": "job-001",
  "action": "copy",
  "source": "/data/input",
  "target": "minio-demo:trailtransfer",
  "recursive": true,
  "filters": ["*.jpg", "*.png"],
  "dry_run": false
}
```

Actions: `copy` · `move` · `sync` · `check` · `cancel` · `status`. There is
deliberately **no** `args`/`flags`/`extra` field — unknown fields are rejected.

## Example result event (sealed evidence envelope)

The result is wrapped in a tamper-evident envelope and chained to the previous
record (full model in [`docs/EVIDENCE_MODEL.md`](docs/EVIDENCE_MODEL.md); sample
in [`examples/events/result.completed.json`](examples/events/result.completed.json)):

```json
{
  "schema_version": "1",
  "worker_id": "worker-01",
  "seq": 7,
  "prev_hash": "sha256:…",
  "event_type": "result",
  "recorded_at": "2026-01-01T10:00:12.5Z",
  "time_source": "system",
  "issuer": { "auth": "self-declared", "identity": "alice" },
  "payload": {
    "action": "copy", "source": "/data/input", "target": "minio-demo:trailtransfer",
    "status": "completed", "files_transferred": 10, "bytes_transferred": 12345678,
    "rclone_exit_code": 0, "policy_version": "1", "policy_hash": "sha256:…",
    "worker_version": "0.2.0", "command_hash": "sha256:…"
  },
  "content_hash": "sha256:…",
  "chain_hash": "sha256:…"
}
```

Verify a whole journal independently: `trailtransfer verify /data/evidence/journal.jsonl`.

## Worker policy — the security boundary

MQTT commands are untrusted input; `config/worker-policy.example.yaml` is
authoritative. Every command is checked before any rclone process is built, and
the policy's SHA-256 (`policy_hash`) is stamped into every result event.

```yaml
worker_id: worker-01
policy_version: "1"
allowed_actions: [copy, move, sync, check]
allowed_sources: [/data/input, /data/reports]
allowed_targets: [sftp-demo:/upload, minio-demo:trailtransfer]
max_file_size_mb: 500
max_parallel_jobs: 2
allow_delete: false
allow_sync_delete: false        # sync is allowlisted but still gated by this
allow_absolute_targets: false
allow_broadcast_commands: false
dry_run_default: true
require_job_id: true
```

Remote **aliases** (`sftp-demo:`) resolve to `rclone.conf`, which the operator
provides and mounts read-only. TrailTransfer never generates it and never logs
it. See [`docs/POLICY.md`](docs/POLICY.md) for every field.

## MQTT topics

| Topic (`{w}` = worker_id, `{j}` = job_id) | Dir | QoS | Retained |
|---|---|---|---|
| `trailtransfer/{w}/commands` | in | 1 | – |
| `trailtransfer/{w}/jobs/{j}/status` | out | 1 | – |
| `trailtransfer/{w}/jobs/{j}/progress` | out | 0 | – |
| `trailtransfer/{w}/jobs/{j}/result` | out | 1 | – |
| `trailtransfer/{w}/jobs/{j}/logs` | out | 0 | – |
| `trailtransfer/{w}/health` | out | 1 | ✓ (+ LWT `offline`) |
| `trailtransfer/{w}/capabilities` | out | 1 | ✓ |

Job lifecycle: `received → rejected` (schema/policy) **or**
`received → accepted → started → progress* → completed | failed | cancelled`.

## CLI

```bash
trailtransfer run [--config config.yaml]     # start the worker
trailtransfer validate-config [--config …]   # load config+policy, exit 0/1
trailtransfer print-capabilities [--config …]# print the capabilities JSON
trailtransfer verify <journal>               # independently verify the evidence chain, exit 0/1
trailtransfer version
```

## Configuration

A YAML config is optional; `TRAILTRANSFER_*` env vars override it (see
`.env.example`). Required: `worker_id`, `mqtt.host`, `policy_file`,
`rclone_config`.

## Security model

MQTT commands are untrusted; the local policy is authoritative. No shell
execution, no arbitrary rclone flags, no secrets in logs, no unrestricted
paths/remotes. Duplicate `job_id`s are deduplicated; `max_parallel_jobs` bounds
concurrency; a JCS hash chain (`content_hash`/`chain_hash`) backs a verifiable audit trail. See
[`SECURITY.md`](SECURITY.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## TrailMQ integration

TrailTransfer executes controlled file jobs; TrailMQ governs and proves them —
registering the worker as an expected system, managing pub/sub rights, capturing
result events as evidence, and rendering transfers on a timeline. It's purely
additive: TrailTransfer runs against any broker on its own.

```
TrailSource   → emits context-rich source events
TrailTransfer → moves files as controlled jobs
TrailMQ       → governs & proves both
```

## Build & test

```bash
go build ./cmd/trailtransfer
go test ./...
```

## Roadmap

- **v0.1** — MQTT worker, `copy/move/sync/check` + `cancel/status`, policy
  validation, safe rclone execution, status/result events, health/capabilities,
  Docker Compose demo.
- **v0.2** — audit evidence: RFC 8785 (JCS) hashing, sealed envelope with
  `seq`/`prev_hash` **hash chain**, append-only journal, `trailtransfer verify`,
  issuer/justification capture, JSON Schemas. *(this)*
- **v0.3** — Ed25519 signatures, RFC 3161 timestamps, file manifest, WORM export,
  live progress, retries.
- **v1.0** — stable schemas, hardened image, full CI, deployment/validation guide.

## License

Apache-2.0 (see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE)). rclone is invoked
as a separate MIT-licensed subprocess and is not a derived work.
