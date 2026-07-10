# Product Model

**TrailTransfer is an MQTT-controlled rclone worker for audit-ready edge file
transfers.** It turns rclone into a policy-guarded, observable, evidence-oriented
transfer worker.

## Core flow

```
MQTT command in
  → policy validation
  → controlled rclone execution (argv, never a shell)
  → progress / status / result out
  → evidence event for audit / review
```

## What it is

- A small **edge worker**, controlled over **MQTT**.
- Constrained by a **local policy** (allowlists + guards).
- A **controlled runner of rclone** — rclone stays the transfer engine.
- A publisher of **status / progress / result / logs / health / capabilities**.
- **Standalone** against any MQTT broker; later **observable/auditable** by TrailMQ.

## What it is not

- Not an rclone replacement, and not its own SMB/SFTP implementation.
- Not an enterprise MFT suite.
- Not a remote shell or a generic file manager.
- Not a cloud service, and not a REST-first file service.
- Not dependent on TrailMQ.

## Why Go + rclone

Go gives static single-binary edge deployment, clean subprocess handling and
`context` cancellation, and a large DevOps/IIoT contributor base. rclone is a
mature, MIT-licensed engine with 40+ backends — TrailTransfer orchestrates it as
a subprocess instead of reimplementing transfers.

## Ecosystem

```
TrailSource   → emits context-rich source events
TrailTransfer → moves files as controlled jobs
TrailMQ       → governs & proves both (optional, additive)
```

See [`CONCEPT.md`](CONCEPT.md) for the full product/OSS strategy and
[`ARCHITECTURE.md`](ARCHITECTURE.md) for the technical blueprint.
