# Security Policy (Model)

> Vulnerability reporting is in [`../SECURITY.md`](../SECURITY.md). This document
> describes the security *model*.

**TrailTransfer must never become a remote shell.** It executes only
allowlisted transfer actions whose parameters passed a local policy. The threat
surface is the **incoming MQTT commands**, treated as untrusted.

## Hard rules

- No `sh -c`, no shell, no arbitrary shell commands.
- No arbitrary rclone args from the MQTT payload — the action→subcommand mapping
  is hard-coded and the command schema is closed (`DisallowUnknownFields`).
- `filters` are validated (no leading `-`, no control chars) before becoming
  `--include` patterns.
- Action / source / target allowlists; `..` traversal rejected.
- `job_id` dedupe (idempotency) — safe under QoS-1 redelivery.
- `context` cancellation for `cancel`; `max_parallel_jobs` bounds concurrency.
- Destructive `sync`/`delete` off unless explicitly enabled in policy.
- No secrets in logs, events or errors — credentials live only in an
  operator-provided, read-only `rclone.conf`; error output is redacted.
- `policy_version` **and** `policy_hash` in every result; `command_hash` +
  a JCS `content_hash`/`chain_hash` chain back a verifiable audit trail
  (`trailtransfer verify`).

## Safe rclone execution

```go
// good — argument vector, context, no shell:
exec.CommandContext(ctx, rcloneBinary, "copy", source, target, "--config", cfgPath)

// forbidden:
exec.Command("sh", "-c", userInput)
```

Args are built as a `[]string` from validated fields only. `--dry-run` is set
deliberately from the effective policy/command decision.

## Threats & mitigations

| Threat | Mitigation |
|---|---|
| Malicious MQTT command | strict schema + policy engine |
| Path traversal | `filepath.Clean` + prefix check; `..` rejected |
| Command injection | argv vector, no shell, no free-form flag field |
| Secret leakage | redaction, remote aliases, `rclone.conf` never logged |
| Destructive sync/delete | `allow_delete`/`allow_sync_delete` default off; dry-run default |
| Replayed `job_id` | dedupe set |
| Uncontrolled parallelism | `max_parallel_jobs` |
| Large-file overload | `max_file_size_mb` / rclone `--max-size` |

## Deployment hardening

Broker auth + ACLs (only authorised senders may publish to `…/commands`),
TLS/mTLS on the broker, a non-root read-only container, a pinned rclone version
(reported in `capabilities`/`result`), and no inbound port on the edge.
