# Migration: FileFlux → TrailTransfer (Rust → Go)

## Summary

FileFlux was a Rust "super-fast-smb-image-uploader": an MQTT-driven worker that
implemented SMB/SFTP uploads itself, with a `start`/`stop` command model and
random per-run job IDs. It has been transformed into **TrailTransfer**, a
**Go** worker that orchestrates **rclone** under a local policy and emits
audit-ready evidence events.

The active runtime is now **Go only**. The Rust sources are retained as a
migration reference under [`legacy/rust/`](../legacy/rust/) and are **not** a
build path (no CI, no Docker stage builds them).

## What changed

| Area | FileFlux (Rust) | TrailTransfer (Go) |
|---|---|---|
| Language / runtime | Rust | Go (`cmd/trailtransfer`, `internal/*`) |
| Transfer | own SMB/SFTP code | rclone as a subprocess (argv, no shell) |
| Command model | `start` / `stop` + `options` | `copy/move/sync/check/cancel/status`, closed schema |
| Job ID | random UUID per run | client-supplied, required, **deduped** |
| Policy | none | `worker-policy.yaml` — the security boundary |
| Concurrency | file-level only | `max_parallel_jobs` at job level |
| rclone.conf | generated from SMB/SFTP env | operator-provided, read-only, never logged |
| Result event | camelCase, ad-hoc | sealed JCS envelope: `seq`/`prev_hash` chain, `content_hash`, `command_hash`, `policy_hash` |
| Topics prefix | `image_uploader` (configurable) | `trailtransfer` |
| Env prefix | mixed (`MQTT_*`, `SMB_*`, `SFTP_*`) | `TRAILTRANSFER_*` |

## Environment variables

Old FileFlux vars (`MQTT_ROOT_TOPIC`, `SMB_*`, `SFTP_*`, `WORKER_ID`, …) are
**removed**, not aliased — the SMB/SFTP-specific ones have no meaning now that
rclone owns transport. Use `TRAILTRANSFER_*` (see `.env.example`). Notable
rename during this task: `TRAILTRANSFER_RCLONE_PATH` → `TRAILTRANSFER_RCLONE_BINARY`.

## Naming

`FileFlux/fileflux/FILEFLUX` → `TrailTransfer/trailtransfer/TRAILTRANSFER` across
code, README, docs, Docker, compose, CI, examples and MQTT topics. The GitHub
repo is still named `FileFlux`; the Go module is `github.com/RainerGewalt/trailtransfer`,
so renaming the repo to `trailtransfer` is the one remaining external step.

## Removed / archived

- **Archived** to `legacy/rust/`: `Cargo.toml`, `Cargo.lock`, `src/*.rs`.
- **Removed** CI: `rust.yml`, `docker-image.yml` (Rust build/test) and
  `docker-publish.yml` (superseded by `docker.yml`). New: `ci.yml`, `docker.yml`.
- Rust `Dockerfile` / `docker-compose.yaml` replaced with Go equivalents.

## Open points

- Rename the GitHub repository `FileFlux` → `trailtransfer`.
- `docs/CONCEPT.md` still contains Rust-worded passages (§1/§7/§16) — de-Rust it.
- Live progress parsing, retries and dead-letter events are v0.2.
- `.idea/` IDE files and the old `create_sftp_test_cases.sh` demo helper were
  left untouched.
