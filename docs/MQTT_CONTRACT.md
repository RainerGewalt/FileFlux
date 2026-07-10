# MQTT Contract

`{w}` = `worker_id`, `{j}` = `job_id`, prefix defaults to `trailtransfer`.

## Topics

| Topic | Direction | QoS | Retained |
|---|---|---|---|
| `trailtransfer/{w}/commands` | worker subscribes | 1 | – |
| `trailtransfer/{w}/jobs/{j}/status` | worker publishes | 1 | – |
| `trailtransfer/{w}/jobs/{j}/progress` | worker publishes | 0 | – |
| `trailtransfer/{w}/jobs/{j}/result` | worker publishes | 1 | – |
| `trailtransfer/{w}/jobs/{j}/logs` | worker publishes | 0 | – |
| `trailtransfer/{w}/health` | worker publishes | 1 | ✓ (+ LWT `offline`) |
| `trailtransfer/{w}/capabilities` | worker publishes | 1 | ✓ |

`trailtransfer/all/commands` (broadcast) is **not implemented** in v0.1: the
worker only ever subscribes to its own command topic. The
`allow_broadcast_commands` policy flag is reserved and defaults to `false`.

## Commands

Actions: `copy` · `move` · `sync` · `check` · `cancel` · `status`. The schema is
**closed** — unknown fields are rejected, and there is no free-form args/flags
field.

```json
{ "job_id": "job-001", "action": "copy", "source": "/data/input",
  "target": "sftp-demo:/upload", "recursive": true,
  "filters": ["*.jpg", "*.png"], "dry_run": false }
```

```json
{ "job_id": "cancel-001", "action": "cancel", "target_job_id": "job-001" }
```

`cancel` and `status` require `target_job_id`. `filters` become rclone
`--include` patterns (validated); they are never passed through raw.

## Job lifecycle

```
received ──(schema fail)──► rejected
   └─(policy fail)────────► rejected
   └─(ok)──► accepted ──► started ──► progress* ──┬──► completed
                                                  ├──► failed
                                                  └──(cancel)──► cancelled
```

Every transition is a `status` event; the terminal state is also a `result`
event. Result `status` is one of `completed | failed | cancelled | rejected`.

## Events

- **status** — `event_type, job_id, worker_id, status, detail?, timestamp`
- **progress** — adds `files_total, files_transferred, bytes_total, bytes_transferred, percent`
- **result** — audit-ready; see [`EXAMPLES.md`](EXAMPLES.md) and below
- **log** — `level, message`
- **health** (retained) — `status, uptime_seconds, active_jobs, max_parallel_jobs, rclone_available, policy_version, policy_hash, supported_actions`
- **capabilities** (retained) — `supported_actions, max_parallel_jobs, rclone_available, policy_version, policy_hash`

Result carries the evidence fields `command_hash`, `result_hash`,
`policy_version`, `policy_hash`, `worker_version`, `rclone_exit_code`.
