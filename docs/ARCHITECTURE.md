# TrailTransfer — Architektur- & Umsetzungsblueprint (Go)

> **Claim:** MQTT-controlled rclone worker for audit-ready edge file transfers.
> **Kernformel:** `MQTT command in → policy validation → controlled rclone execution → progress/status/result out → evidence event`.
> **Leitsatz:** Works with any MQTT broker. Best with TrailMQ.

Dieses Dokument ist der **technische Bauplan** für die Schärfung von FileFlux zu TrailTransfer als **Go-Worker**. Die Produkt-/OSS-Strategie (Positionierung, Zielgruppen, Lizenz, Monetarisierung, Risiken) ist bereits ausführlich in [`CONCEPT.md`](./CONCEPT.md) beschrieben und wird hier nur verdichtet referenziert. Der Fokus liegt auf **Architektur, Schemata, Policy, sicherer rclone-Ausführung, Docker, Tests, CI und dem konkreten Migrationspfad** aus dem aktuellen (halb migrierten, Rust-basierten) Stand.

Sprache: deutsche Erläuterung, englische Ship-Copy und Code (Repo-Artefakte werden englisch ausgeliefert).

---

## 1. Produktdiagnose: FileFlux → TrailTransfer (Ist-Zustand im Code)

Der Rename ist **begonnen, aber nicht konsistent**. Der Code steht in einem Zwischenzustand, der weder das alte noch das neue Produkt sauber abbildet:

| Baustein | Status im Repo | Bewertung |
|---|---|---|
| `Cargo.toml` | `name = "trailtransfer"`, v0.1.0, Claim als `description` | ✅ passt schon |
| `config.rs` | hat `worker_id` + Topic-Helper (`command_topic`, `health_topic`, `job_*_topic`) im **neuen** Schema | ✅ Topic-Modell stimmt; ❌ Config selbst ist noch **SMB/SFTP-env-gekoppelt** (`smb_target_ip`, `sftp_host`, …), kein `policy_file`, kein `rclone_config`-Pfad |
| `rclone.rs` | ruft rclone als **Subprozess mit Argument-Vektor** (kein Shell!), parst `--use-json-log --stats` | ✅ richtiges Muster; ❌ **generiert** `rclone.conf` aus SMB/SFTP-ENV via `rclone obscure` und kennt nur `smb_remote`/`sftp_remote` — das ist FileFlux-DNA, kein policy-getriebener rclone-Worker |
| `mqtt_service.rs` | Command-Schema ist noch **alt**: `action: "start"/"stop"`, `options{transfer_type, recursive_folders, files, file_filters, transfer_strategy}`, `job_id` wird als **zufällige UUID** erzeugt | ❌ Kein `copy/move/sync/check/cancel`, kein `job_id` aus dem Command, **keine Policy-Prüfung**, kein Dedupe möglich (UUID = jeder Job „neu"). LWT ist ✅ implementiert |
| `transfer.rs` | Result-JSON in **camelCase** (`jobId`, `engine`, `throughputMbPerSec`) | ❌ nicht das audit-ready Schema (snake_case + `command_hash`/`result_hash`/`policy_version`/`worker_version`) |
| Job-Parallelität | nur **datei-level** Semaphore (max 5 Dateien/Job); **kein Job-Level-Limit** → unbegrenzte `tokio::spawn` pro `start` | ❌ `max_parallel_jobs` fehlt (Blast-Radius/DoS) |
| `Dockerfile` | `FROM rust:1.83`, kopiert Binary **`super-fast-smb-image-uploader`** (existiert nicht mehr), Runtime **ohne rclone** | ❌ kaputt für das neue Produkt |
| `docker-compose.yaml` | Service `uploader`, `MQTT_ROOT_TOPIC=image_uploader`, alte ENV, kein `/config`-Mount, kein MinIO | ❌ stale |
| `docs/CONCEPT.md` | exzellente 40 KB Produkt-/OSS-Strategie | ✅ inhaltlich stark; ⚠️ **Rust-orientiert** (rumqttc, `Command::new`, musl/distroless) |

**Diagnose in einem Satz:** Das *Design* ist bereits TrailTransfer (Topics, Claim, Subprozess-Muster, LWT), aber die *Substanz* ist noch FileFlux (start/stop-Schema, SMB/SFTP-ENV-Kopplung, generierte `rclone.conf`, keine Policy, keine Evidence) — und die Zielsprache soll von Rust auf **Go** wechseln.

### 1a. Die eigentliche Entscheidung: Go vs. Rust (ehrlich)

Der Brief empfiehlt Go. Das ist **vertretbar**, aber die Begründung muss stimmen, sonst trifft man die Entscheidung aus dem falschen Grund:

- ❌ **„rclone ist Go" ist *kein* tragfähiges Argument.** rclone wird als **Subprozess** aufgerufen — in Rust *und* Go identisch. Es als Go-Library einzubinden ist ausdrücklich **nicht** gewünscht (bläht das Binary auf, koppelt an rclone-Interna, widerspricht dem „versionierte Engine als Subprozess"-Design). Sprachwahl ändert an der rclone-Integration **nichts**.
- ✅ **Der echte Grund ist OSS-/Ökosystem-Fit.** Go hat im DevOps/IIoT/Cloud-Native-Umfeld den mit Abstand größeren Contributor-Pool und niedrigere Einstiegshürde — genau die Zielgruppe. Statische Binaries (`CGO_ENABLED=0`) und Docker sind minimal einfacher als Rust-musl. Und: die Arbeitsumgebung ist bereits Go-zentriert (`GolandProjects/`, `.idea/go.imports.xml`).
- ⚖️ **Die Kosten sind niedriger als sie wirken.** Der bestehende Rust-Code ist klein (~7 Dateien) und müsste für das neue Design **ohnehin größtenteils neu geschrieben** werden (Command-Schema, Policy, rclone.conf-Handling, Evidence). Wiederverwendbar ist nur *Wissen* (Topic-Layout, „arg-vector statt Shell", JSON-Stats-Parsing) — das ist sprachunabhängig und in Go in Tagen reproduziert.

**Empfehlung:** **Go umsetzen** — mit einer Bedingung: Wenn TrailMQ/TrailSource in **Rust** geschrieben sind, sollte man wegen geteilter Contract-/Evidence-Libraries und Maintainer-Velocity noch einmal innehalten. Ist das Ökosystem Go (oder sprach-agnostisch über MQTT-Contracts gekoppelt), ist Go die richtige Wahl. Das „rclone ist Go"-Argument bitte **nicht** als Begründung führen.

> Ab hier beschreibt das Dokument die **Go-Zielarchitektur**. `CONCEPT.md` sollte in einem Folgeschritt an die Go-Sprachwahl angepasst werden (Abschnitt 7/14/Anhang: rumqttc→Paho, `Command::new`→`os/exec`, musl→`CGO_ENABLED=0`).

---

## 2. Finale Produktpositionierung

Unverändert gegenüber `CONCEPT.md` §3 — hier als Disziplin-Anker:

TrailTransfer **ist**: MQTT-controlled edge transfer worker · policy-guarded rclone runner · audit-ready job executor · TrailMQ-native worker.

TrailTransfer ist **nicht**: „better rclone" · „file uploader" · „enterprise MFT" · „REST file transfer API" · „generic/remote command executor" · Cloud-Service.

```
TrailTransfer = MQTT command in
              → policy validation
              → controlled rclone execution
              → progress / status / result out
              → evidence event for audit
```

**Regel:** Sobald TrailTransfer irgendwo als „Uploader" oder „rclone-Wrapper" beschrieben wird, ist die Positionierung verloren. Der Wert ist **Steuerbarkeit + Nachweisbarkeit**, nicht „Dateien schieben".

**Use Cases** (Kurzform, Details in `CONCEPT.md` §4): Maschinenbilder Edge→SFTP/NAS/S3/MinIO · Prüfberichte aus Produktion abholen/verschieben · Logs/Diagnosepakete sichern · CSV/JSON/XML-Exports automatisiert transportieren · Batch-/Auftragsdaten aus Linienumgebungen · lokale Ordner kontrolliert in zentrale Speicher spiegeln · alles in TrailMQ auditieren. **Nie:** offene REST-API am Edge, unsichtbare Cron/SSH/Shell-Skripte.

---

## 3. Architekturvorschlag

```
                 ┌──────────────────────────────┐
                 │   MQTT Broker / TrailMQ       │  auth · ACL · contracts · LWT
                 └──────────────┬───────────────┘
                                │  command  trailtransfer/{worker_id}/commands   (QoS1)
                                ▼
   ┌──────────────────────────────────────────────────────────────┐
   │                     TrailTransfer Worker (Go)                  │
   │                                                                │
   │  mqtt ── decode+strict-schema ──► commands ──► policy ──► jobs │
   │   ▲            │ reject → result             (allowlist)   │   │
   │   │            └────────────────────────────────────────┐  │  │
   │  events ◄── evidence ◄── result ◄── rclone runner ◄──────┘  │  │
   │  (status/progress/result/logs/health/capabilities)   argv   │  │
   └──────────────────────────────────────────────────────┼──────┘
                                                           │ exec (arg vector, no shell)
                                                           ▼
                 ┌──────────────────────────────┐
                 │            rclone             │  ◄── rclone.conf (secrets, RO mount)
                 │       the transfer engine     │
                 └──────────────┬───────────────┘
                    SMB · SFTP · S3/MinIO · WebDAV · NAS · SharePoint · …
```

**Interne Pakete (Verantwortungsschnitt):**

- `internal/config` — ENV/YAML laden, validieren, sichere Defaults, `TRAILTRANSFER_*`-Präfix.
- `internal/mqtt` — **eine** ausgehende Verbindung (Paho), Auto-Reconnect, QoS, retained Health/Capabilities, LWT, Publish/Subscribe-Wrapper.
- `internal/commands` — JSON→Struct **mit strikter Dekodierung** (`DisallowUnknownFields`), typisierte Command-Modelle, Validierung des Envelope.
- `internal/policy` — `worker-policy.yaml` laden, `policy_version`, Allowlists, Guards; liefert `Decision{Allowed, Reason, Detail}`.
- `internal/jobs` — Job-Zustandsmaschine, `job_id`-Dedupe, `max_parallel_jobs`-Semaphore, Cancel via `context`.
- `internal/rclone` — **Action→argv-Mapping** (hart codiert), Subprozess, `--use-json-log`/`--stats`-Parsing, Exit-Code-Kapselung.
- `internal/events` — kanonische Event-Structs + Publish auf die Topics.
- `internal/evidence` — kanonische Serialisierung, `command_hash`/`result_hash`, Anreicherung mit Versionen.
- `internal/health` — Heartbeat-Loop, Capabilities-Publish.
- `internal/logging` — strukturierte Logs (`slog`), **Redaction** von Secrets/Credentials.

Datenfluss eines Jobs: `received → (schema) → (policy) → accepted → started → progress* → completed|failed` bzw. `received → rejected` (Policy). Jeder Übergang = ein MQTT-Event.

---

## 4. Go-Projektstruktur

```
cmd/trailtransfer/main.go            # CLI: run | validate-config | print-capabilities | version

internal/
  config/       config.go  config_test.go        # ENV+YAML, defaults, validation
  mqtt/         client.go                         # Paho wrapper, reconnect, LWT, QoS
  commands/     command.go  decode.go  decode_test.go
  policy/       policy.go  validate.go  validate_test.go
  jobs/         manager.go  job.go  state.go  manager_test.go
  rclone/       runner.go  argv.go  argv_test.go  stats.go  stats_test.go
  events/       events.go  publisher.go
  evidence/     hash.go  hash_test.go  canonical.go
  health/       health.go
  logging/      logging.go  redact.go  redact_test.go

examples/
  commands/     copy.json  sync-dry-run.json  cancel.json  check.json
  worker-policy.yaml
  rclone.conf.example
  mosquitto.conf

docs/            ARCHITECTURE.md (dieses) · CONCEPT.md · security-model.md · mqtt-api.md · …
Dockerfile
docker-compose.yml
.env.example
README.md
LICENSE  NOTICE  SECURITY.md  CONTRIBUTING.md  CHANGELOG.md
.github/workflows/  ci.yml  docker.yml
```

`go.mod`: `module github.com/<org>/trailtransfer` · Go 1.22+ (`slog`, `log/slog`).

**CLI-Gerüst (std `flag`, kein Cobra nötig für V1):**

```go
func main() {
    if len(os.Args) < 2 { usage(); os.Exit(2) }
    switch os.Args[1] {
    case "run":                cmdRun(os.Args[2:])
    case "validate-config":    cmdValidateConfig(os.Args[2:])   // lädt config+policy, exit 0/1
    case "print-capabilities": cmdPrintCapabilities(os.Args[2:])// druckt Capabilities-JSON
    case "version":            fmt.Println(version.String())    // von -ldflags injiziert
    default:                   usage(); os.Exit(2)
    }
}
```

---

## 5. MQTT Topic Model

Basis (`{worker_id}` pro Instanz eindeutig):

```
SUBSCRIBE  trailtransfer/{worker_id}/commands

PUBLISH    trailtransfer/{worker_id}/health                    (retained + LWT)
PUBLISH    trailtransfer/{worker_id}/capabilities              (retained)
PUBLISH    trailtransfer/{worker_id}/jobs/{job_id}/status
PUBLISH    trailtransfer/{worker_id}/jobs/{job_id}/progress
PUBLISH    trailtransfer/{worker_id}/jobs/{job_id}/result
PUBLISH    trailtransfer/{worker_id}/jobs/{job_id}/logs
```

**QoS / Retention / LWT — Designregeln:**

| Topic | QoS | Retained | Begründung |
|---|---|---|---|
| `commands` | 1 | – | at-least-once; Duplikate über `job_id`-Idempotenz abgefangen |
| `status` | 1 | – | verlässlicher Verlauf |
| `result` | 1 | – | **die Quittung** — muss ankommen |
| `progress` | 0 | – | Volumen, verlustbar |
| `logs` | 0 | – | Volumen, verlustbar |
| `health` | 1 | ✅ + **LWT** | neue Subscriber kennen Zustand sofort; LWT setzt `{"status":"offline"}` bei Verbindungsverlust |
| `capabilities` | 1 | ✅ | neue Subscriber kennen Fähigkeiten sofort |

**`trailtransfer/all/commands` (Broadcast) — kritisch bewertet:** In **V1 nicht implementieren**. Ein Broadcast-Command-Kanal ist ein Flotten-weiter Trigger und damit ein hochwertiges Angriffs-/Fehlbedienungsziel. Falls später überhaupt: (a) **default deaktiviert** (`enable_broadcast: false` in Policy), (b) **nur** nicht-destruktive Actions (`check`, `status`), **nie** `copy/move/sync/delete`, (c) idealerweise nur mit signiertem Command + TrailMQ-Autorisierung. Der Normalfall bleibt gezieltes Adressieren pro `worker_id`.

---

## 6. Command Schemas

Aktionen: `copy · move · sync · check · cancel · status`. **Regeln:** `job_id` Pflicht · `action` muss erlaubt sein · `source`/`target` gegen Policy · **keine freien Shell-Argumente, keine arbitrary rclone-Flags, keine Shell-Ausführung, keine Command-Injection**.

**copy**
```json
{
  "job_id": "job-001",
  "action": "copy",
  "source": "/data/input",
  "target": "sftp-demo:/upload",
  "recursive": true,
  "filters": ["*.jpg", "*.png"],
  "dry_run": false
}
```

**sync (dry-run)** — `sync` ist destruktiv (Zielseite kann gelöscht werden), daher nur mit `allow_sync_delete` und bevorzugt zuerst `dry_run`:
```json
{ "job_id": "job-014", "action": "sync", "source": "/data/reports", "target": "minio-demo:trailtransfer/reports", "dry_run": true }
```

**check** — Integritätsvergleich Quelle/Ziel ohne Transfer:
```json
{ "job_id": "job-020", "action": "check", "source": "/data/input", "target": "minio-demo:trailtransfer" }
```

**cancel** — bricht laufenden Job ab (Feldname konsistent zum Brief: `target_job_id`):
```json
{ "job_id": "cancel-001", "action": "cancel", "target_job_id": "job-001" }
```

**status** — fragt aktuellen Zustand eines Jobs ab (re-publiziert `status`-Event):
```json
{ "job_id": "q-001", "action": "status", "target_job_id": "job-001" }
```

**Go-Modell + strikte Dekodierung** (unbekannte Felder = Ablehnung; kein Feld ist ein Freitext-Flag-Container):

```go
type Action string

const (
    ActionCopy   Action = "copy"
    ActionMove   Action = "move"
    ActionSync   Action = "sync"
    ActionCheck  Action = "check"
    ActionCancel Action = "cancel"
    ActionStatus Action = "status"
)

type Command struct {
    JobID       string   `json:"job_id"`
    Action      Action   `json:"action"`
    Source      string   `json:"source,omitempty"`
    Target      string   `json:"target,omitempty"`
    Recursive   bool     `json:"recursive,omitempty"`
    Filters     []string `json:"filters,omitempty"`
    DryRun      bool     `json:"dry_run,omitempty"`
    TargetJobID string   `json:"target_job_id,omitempty"` // for cancel/status
}

func Decode(payload []byte) (*Command, error) {
    dec := json.NewDecoder(bytes.NewReader(payload))
    dec.DisallowUnknownFields()               // reject anything not in the schema
    var c Command
    if err := dec.Decode(&c); err != nil {
        return nil, fmt.Errorf("schema: %w", err)
    }
    if c.JobID == "" {
        return nil, errors.New("schema: job_id is required")
    }
    if !c.Action.valid() {
        return nil, fmt.Errorf("schema: unknown action %q", c.Action)
    }
    return &c, nil
}
```

`filters` werden **nicht** roh durchgereicht, sondern als rclone `--include`-Pattern gemappt (Abschnitt 10). Es gibt **kein** `args`/`extra`/`flags`-Feld — bewusst.

---

## 7. Policy YAML

`examples/worker-policy.yaml` — **die Sicherheitsgrenze.** MQTT darf nie zum Remote-Shell-Kanal werden.

```yaml
worker_id: worker-01
policy_version: "1"

allowed_actions:
  - copy
  - move
  - sync
  - check

allowed_sources:
  - /data/input
  - /data/reports

allowed_targets:
  - sftp-demo:/upload
  - minio-demo:trailtransfer

max_file_size_mb: 500
max_parallel_jobs: 2

allow_delete: false            # delete/purge actions
allow_sync_delete: false       # sync darf Zielseite löschen
allow_absolute_targets: false  # absolute Zielpfade
dry_run_default: true          # Jobs ohne dry_run-Feld laufen als Trockenlauf
require_job_id: true
enable_broadcast: false        # trailtransfer/all/commands (siehe §5)
```

**Validierungen (jede einzeln, mit sauberem `rejected`-Result — nie stiller Fehlschlag):**

1. `action` ∈ `allowed_actions`?
2. `source` unter einem `allowed_sources`-Präfix? (nach Pfad-Normalisierung, siehe §10 gegen Traversal)
3. `target` ∈ `allowed_targets` bzw. unter erlaubtem Remote+Pfad?
4. `sync`/`delete` überhaupt erlaubt? (`allow_sync_delete`/`allow_delete`)
5. absolute Ziele erlaubt? (`allow_absolute_targets`)
6. Dateigröße ≤ `max_file_size_mb`? (rclone `--max-size`, zusätzlich Vorabprüfung)
7. `max_parallel_jobs` nicht überschritten? (sonst `rejected: too_many_jobs`)
8. `dry_run` gesetzt — sonst `dry_run_default` anwenden
9. `job_id` vorhanden (`require_job_id`)?
10. `job_id` nicht bereits gesehen? (Dedupe)

```go
type Decision struct { Allowed bool; Reason, Detail string }

func (p *Policy) Evaluate(c *commands.Command) Decision {
    if !p.actionAllowed(c.Action) {
        return Decision{false, "policy_violation", fmt.Sprintf("action %q not allowed", c.Action)}
    }
    if !p.sourceAllowed(c.Source) {
        return Decision{false, "policy_violation", fmt.Sprintf("source %q not in allowed_sources", c.Source)}
    }
    if !p.targetAllowed(c.Target) {
        return Decision{false, "policy_violation", fmt.Sprintf("target %q not in allowed_targets", c.Target)}
    }
    if c.Action == commands.ActionSync && !p.AllowSyncDelete {
        return Decision{false, "policy_violation", "sync delete not permitted"}
    }
    return Decision{Allowed: true}
}
```

**Betrieb:** Policy in Versionskontrolle, `policy_version` bei jeder Änderung hochziehen — sie erscheint in jedem Result-Event und macht Änderungen später zuordenbar.

---

## 8. Job Lifecycle

**V0.1-Zustände:** `received · rejected · accepted · started · progress · completed · failed · cancelled`.
**Später:** `validated · queued · retrying · timed_out · dead_letter`.

```
received ──(schema fail)──► rejected
   │
   └─(policy fail)────────► rejected
   │
   └─(ok)──► accepted ──► started ──► progress* ──┬──► completed
                                                  ├──► failed
                                                  └──(cancel)──► cancelled
```

Jeder Zustand = ein `status`-Event (QoS 1). Feldname **`status`** durchgängig (Achtung: `CONCEPT.md` nutzt teils `state` im Status-Event — hier auf **`status`** vereinheitlichen, konsistent mit `result`).

**status event:**
```json
{
  "event_type": "status",
  "job_id": "job-001",
  "worker_id": "worker-01",
  "status": "started",
  "timestamp": "2026-01-01T10:00:00Z"
}
```

**progress event:**
```json
{
  "event_type": "progress",
  "job_id": "job-001",
  "worker_id": "worker-01",
  "files_total": 10,
  "files_transferred": 4,
  "bytes_total": 12345678,
  "bytes_transferred": 5000000,
  "percent": 40.5,
  "eta_seconds": 7,
  "timestamp": "2026-01-01T10:00:07Z"
}
```

---

## 9. Result / Evidence Schema

Das finale Result-Event ist die Grundlage für TrailMQ Evidence/Audit. **snake_case, kanonisch hashbar.**

```json
{
  "event_type": "result",
  "job_id": "job-001",
  "worker_id": "worker-01",
  "action": "copy",
  "source": "/data/input",
  "target": "sftp-demo:/upload",
  "status": "completed",
  "started_at": "2026-01-01T10:00:00Z",
  "finished_at": "2026-01-01T10:00:12Z",
  "duration_ms": 12000,
  "files_total": 10,
  "files_transferred": 10,
  "bytes_transferred": 12345678,
  "errors": [],
  "rclone_exit_code": 0,
  "policy_version": "1",
  "worker_version": "0.1.0",
  "command_hash": "sha256:…",
  "result_hash": "sha256:…"
}
```

**Rejected-Result** (Policy-Verstoß — nie stiller Fehlschlag):
```json
{
  "event_type": "result",
  "job_id": "job-013",
  "worker_id": "worker-01",
  "status": "rejected",
  "reason": "policy_violation",
  "detail": "target 'sftp-prod:/etc' not in allowed_targets",
  "policy_version": "1",
  "worker_version": "0.1.0",
  "command_hash": "sha256:…"
}
```

**Evidence-Regeln:**
- `command_hash` = SHA-256 über das **kanonische** empfangene Command (sortierte Keys, kein Whitespace). Beweist *welche* Anweisung lief.
- `result_hash` = SHA-256 über das kanonische Result **ohne** das `result_hash`-Feld selbst. Beweist Ergebnis-Integrität.
- Kanonisierung ist **Pflicht** — ohne sie sind Hashes wertlos. Früh einbauen (auch wenn „offiziell" erst v0.3), nachträglich ist es teuer.
- Später (v0.3+): optionales `manifest` (Datei + Größe + Hash) und Ed25519-Signatur des Events.

```go
// canonical.go — deterministische Serialisierung (sortierte Keys)
func Canonical(v any) ([]byte, error) {
    raw, err := json.Marshal(v)
    if err != nil { return nil, err }
    var m map[string]any
    if err := json.Unmarshal(raw, &m); err != nil { return nil, err }
    // encoding/json marshalt map-Keys sortiert → deterministisch
    return json.Marshal(m)
}

func Hash(v any) (string, error) {
    b, err := Canonical(v)
    if err != nil { return "", err }
    sum := sha256.Sum256(b)
    return "sha256:" + hex.EncodeToString(sum[:]), nil
}
```

---

## 10. Sichere rclone-Ausführung

**Grundsatz:** TrailTransfer ist **kein** Remote-Shell-Executor. Kein `sh -c`, kein interpolierter String, keine Command-Injection.

```go
// FALSCH — niemals:
exec.Command("sh", "-c", userInput)

// RICHTIG — Argument-Vektor, Action hart gemappt:
ctx, cancel := context.WithCancel(parent)         // cancel für Cancel-Jobs
cmd := exec.CommandContext(ctx, cfg.RclonePath, argv...)
```

**Action → argv-Mapping (hart codiert, whitelist-only):**

```go
func BuildArgs(cfg Config, c *commands.Command, dryRun bool) ([]string, error) {
    var sub string
    switch c.Action {
    case commands.ActionCopy:  sub = "copyto"
    case commands.ActionMove:  sub = "moveto"
    case commands.ActionSync:  sub = "sync"     // nur wenn Policy allow_sync_delete
    case commands.ActionCheck: sub = "check"
    default:
        return nil, fmt.Errorf("action %q not executable", c.Action)
    }
    args := []string{
        sub, c.Source, c.Target,
        "--config", cfg.RcloneConfig,
        "--use-json-log", "--stats", "500ms", "--stats-log-level", "NOTICE",
        "--max-size", fmt.Sprintf("%dM", cfg.MaxFileSizeMB),
    }
    if dryRun { args = append(args, "--dry-run") }
    for _, f := range c.Filters {                 // aus Schema, nicht roh: als --include
        if err := validateFilterPattern(f); err != nil { return nil, err }
        args = append(args, "--include", f)
    }
    return args, nil
}
```

**Regeln:** keine Shell · keine freien Argumente aus MQTT · Action-Mapping hart · `source`/`target` **vor** dem Bauen gegen Policy validiert (+ Pfad-Normalisierung `filepath.Clean` und Präfix-Check gegen Traversal `../`) · `context` für Cancel · stdout `null`, stderr **zeilenweise** gelesen und geparst · Exit-Code erfasst · Timeout optional (`context.WithTimeout`) · Logs begrenzt · **keine Secrets loggen**.

**Stats-Parsing** (rclone `--use-json-log`): jede stderr-Zeile ist ein JSON-Log; Objekte mit `stats{bytes, transfers, totalBytes, …}` → Progress, letzter Stats-Wert → Totale.

```go
sc := bufio.NewScanner(stderr)
for sc.Scan() {
    var line struct{ Stats *struct{
        Bytes, TotalBytes int64; Transfers, TotalTransfers int
    } `json:"stats"` }
    if json.Unmarshal(sc.Bytes(), &line) == nil && line.Stats != nil {
        onProgress(line.Stats.Bytes, line.Stats.TotalBytes, line.Stats.Transfers)
    }
}
_ = cmd.Wait()
exit := cmd.ProcessState.ExitCode()  // in result.rclone_exit_code
```

**rclone.conf ≠ generiert.** Anders als der aktuelle Rust-Stand (der die conf aus SMB/SFTP-ENV baut): die `rclone.conf` wird vom **Operator** bereitgestellt und **read-only** unter `/config` gemountet. Sie enthält die Secrets; TrailTransfer liest sie nur, loggt sie nie, und kennt Remotes ausschließlich als **Aliase** (`sftp-demo:`), die die Policy auf der Allowlist führt. Das entkoppelt TrailTransfer von konkreten Protokollen und macht alle 40+ rclone-Backends nutzbar, ohne Code zu ändern.

---

## 11. Docker / Compose-Konzept

**Dockerfile** (Multi-stage Go, rclone im Runtime, non-root, gepinnt):

```dockerfile
# ---- build ----
FROM golang:1.22 AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
ARG VERSION=dev
RUN CGO_ENABLED=0 go build -trimpath \
    -ldflags "-s -w -X main.version=${VERSION}" \
    -o /out/trailtransfer ./cmd/trailtransfer

# ---- rclone (gepinnte Version) ----
FROM rclone/rclone:1.66 AS rclone

# ---- runtime ----
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build   /out/trailtransfer /usr/local/bin/trailtransfer
COPY --from=rclone  /usr/local/bin/rclone /usr/local/bin/rclone
USER nonroot:nonroot
VOLUME ["/config", "/data"]
ENTRYPOINT ["/usr/local/bin/trailtransfer"]
CMD ["run", "--config", "/config/config.yaml"]
```

> rclone-Version im `capabilities`/`result`-Event ausweisen und hier pinnen (Versions-Drift ist ein reales Risiko). rclone bleibt MIT-lizenziert und ist **nicht Teil des eigenen Distributionsartefakts im Quellsinn** — es wird zur Laufzeit als Subprozess aufgerufen (NOTICE-Hinweis).

**docker-compose.yml** (Ein-Kommando-Demo — ersetzt die stale `docker-compose.yaml`):

```yaml
services:
  trailtransfer:
    build: { context: ., args: { VERSION: dev } }
    depends_on: [mqtt, minio]
    environment:
      TRAILTRANSFER_WORKER_ID: worker-01
      TRAILTRANSFER_MQTT_HOST: mqtt
      TRAILTRANSFER_MQTT_PORT: "1883"
      TRAILTRANSFER_POLICY_FILE: /config/worker-policy.yaml
      TRAILTRANSFER_RCLONE_CONFIG: /config/rclone.conf
    volumes:
      - ./examples:/config:ro          # worker-policy.yaml + rclone.conf
      - ./upload_folder:/data/input:ro # sample input

  mqtt:
    image: eclipse-mosquitto:2
    ports: ["1883:1883"]
    volumes: ["./examples/mosquitto.conf:/mosquitto/config/mosquitto.conf:ro"]

  minio:
    image: minio/minio
    command: server /data --console-address ":9001"
    environment: { MINIO_ROOT_USER: minio, MINIO_ROOT_PASSWORD: minio12345 }
    ports: ["9000:9000", "9001:9001"]
```

Ziel: `docker compose up` → `copy.json` auf `trailtransfer/worker-01/commands` publizieren → Result-Event auf `.../jobs/job-001/result` beobachten. (SFTP-Alternative `atmoz/sftp` statt MinIO möglich — die vorhandenen `create_sftp_test_cases.sh`/`sftp_*`-Mounts lassen sich wiederverwenden.)

---

## 12. README Draft

Struktur (Ship-Copy englisch, vollständiger Entwurf in `CONCEPT.md` §15):

```markdown
# TrailTransfer
**MQTT-controlled rclone worker for audit-ready edge file transfers.**

Edge systems often need to move files without exposing APIs, opening SSH, or
relying on invisible cron jobs. TrailTransfer lets you control file transfers
through MQTT, enforce a local transfer policy, execute rclone safely, and
publish structured status / progress / result / evidence events.

TrailTransfer does **not** replace rclone — rclone stays the transfer engine.
TrailTransfer is the MQTT control plane, policy layer and evidence wrapper.
> Works with any MQTT broker. Best with TrailMQ.

## What it is / is not   ## Quickstart (`docker compose up`)   ## Example command
## Policy   ## MQTT Topics   ## Security Model   ## TrailMQ Integration   ## Roadmap
```

Der aktuelle README (`# FileFlux … SMB/SFTP image uploader`) muss vollständig ersetzt werden — er beschreibt noch das alte Produkt inkl. `MQTT_ROOT_TOPIC=image_uploader`, SMB via `smbclient`, Kompression usw.

---

## 13. Security Model

**Prinzipien:** MQTT-Commands sind **untrusted input** · lokale Policy ist **autoritativ** · keine Shell · keine arbitrary args · keine Secrets in Logs · keine unrestricted paths/remotes · least-privilege Container (non-root, read-only rootfs, `/config` RO) · `dry_run` default optional · Duplicate-Job-Schutz · Audit-Hashes für Command/Result · TLS/mTLS-MQTT später empfohlen.

| Threat | Gegenmaßnahme |
|---|---|
| malicious MQTT command | strikte Schema-Validierung + Policy-Engine |
| path traversal | `filepath.Clean` + Präfix-Check gegen `allowed_sources`, kein `..` |
| command injection | Argument-Vektor, kein Shell-Eval, kein Freitext-Flag-Feld |
| secret leakage | Redaction, Remote-Aliase statt Credentials, `rclone.conf` nie geloggt |
| destructive sync/delete | `allow_delete`/`allow_sync_delete` default **off**, `--dry-run` default |
| replayed job_id | `job_id`-Dedupe (Idempotenz-Set), schützt bei QoS-1-Redelivery |
| uncontrolled parallel jobs | `max_parallel_jobs`-Semaphore → `rejected: too_many_jobs` |
| large file overload | `max_file_size_mb` / rclone `--max-size` |

**Angriffsfläche = eingehende MQTT-Commands.** Verteidigung = Broker-Auth/ACL (bzw. TrailMQ-Rechte) + Schema + Policy + kein Shell-Eval + **kein offener Inbound-Port am Edge**. → eigenes `docs/security-model.md` (wichtigster Vertrauensbaustein für OT).

---

## 14. Teststrategie

**Unit:** command parsing (inkl. `DisallowUnknownFields` lehnt Extra-Felder ab) · policy allow/deny je Dimension (action/source/target) · duplicate `job_id` · `dry_run_default`-Anwendung · **argv-Konstruktion** (Snapshot: erwarteter Arg-Slice pro Action) · result-Event-Generierung · **Hash-Determinismus** (gleiche Semantik ⇒ gleicher Hash; `result_hash` schließt sich selbst aus) · Redaction (kein Secret im Log-Output).

**Integration:** MQTT-Command → `accepted`/`rejected`-Event (Mosquitto-Container) · copy-Job gegen lokales rclone-Ziel (`:local:`-Remote / temp dir) · cancel-Job (Context-Abbruch) · fehlerhafter rclone-Exit-Code → `failed`-Result · Policy-Ablehnung → `rejected`-Result · Compose-Smoke-Test.

**Besonders wertvoll für dieses Produkt:** ein Test, der beweist, dass **kein** Command-Feld je in einer Shell landet, und einer, der die **Kanonisierung** der Hashes absichert — das sind die USP-tragenden Eigenschaften.

**CI-Gates:** `gofmt -l` (leer) · `go vet ./...` · `golangci-lint` (empfohlen) · `go test ./... -race` · `go build ./...` · `docker build`.

---

## 15. CI/CD-Vorschlag

Drei alte Workflows (`docker-image.yml`, `docker-publish.yml`, `rust.yml`) auf **zwei** konsolidieren:

**`ci.yml`** (on push / PR): `gofmt`-Check → `go vet` → `golangci-lint` → `go test -race ./...` → `go build`. Optional `govulncheck`.

**`docker.yml`** (on tag `v*`): `docker buildx` multi-arch (`linux/amd64`, `linux/arm64`, optional `linux/arm/v7` für RPi/Edge) → Push nach **GHCR** mit Tags `vX.Y.Z` **und** `latest` → nur `GITHUB_TOKEN`, keine Langzeit-Secrets. `VERSION` als Build-Arg → `-ldflags -X main.version` (erscheint in jedem Result-Event). SBOM/Provenance optional (`--sbom=true --provenance=true`).

---

## 16. TrailMQ Integration

**TrailTransfer executes controlled file jobs. TrailMQ governs and proves them.**

TrailTransfer ist als idealer TrailMQ-**Participant** gebaut:

- **subscribes:** `trailtransfer/{worker_id}/commands`
- **publishes:** `health` · `capabilities` · `jobs/+/status` · `jobs/+/progress` · `jobs/+/result` · `jobs/+/logs`

TrailMQ kann darauf: Worker als **erwartetes System** registrieren · Pub/Sub-**Rechte** verwalten (wer darf Commands senden / Results lesen) · Command-Topics **kontrollieren** · Result-Events als **Evidence** speichern · Transfers in der **Timeline** darstellen · failed jobs sichtbar machen · Policy-Kontext (`policy_version`) speichern · **Export-Reports** erzeugen.

**Contract-Idee:** TrailMQ definiert für `…/commands` ein erwartetes Schema (`action ∈ allowlist`, `job_id` required) und für `…/result` ein Evidence-Schema (Hashes + Versionen Pflicht). Verstöße werden sichtbar, nicht still.

**Harte Regel:** keine Abhängigkeit. Ohne TrailMQ läuft alles gegen jeden Broker; TrailMQ ist rein additiv. CI sollte gegen Vanilla-Mosquitto testen, damit das Standalone-Versprechen nicht schleichend bricht.

```
TrailSource   → emits context-rich source events
TrailTransfer → moves files as controlled jobs
TrailMQ       → governs & proves both
```

---

## 17. Roadmap

- **v0.1 — Walking skeleton:** Go-Worker · MQTT subscribe commands · `copy`/`move`/`check`/`cancel` minimal · Policy-Validierung · rclone-Ausführung · status/result-Events · health/capabilities · Docker-Compose-Demo · README · Apache-2.0.
- **v0.2 — Robust jobs:** Progress-Parsing · Retries · bessere Logs · dead-letter-Events · Metriken · mehr Tests.
- **v0.3 — Evidence:** kanonische command/result-Hashes „offiziell" · Manifest · Policy-Versionierung · TrailMQ-Beispiel-Contracts · signierte Events.
- **v0.4 — Observability & scale:** optionale rclone-`rc`-API · Multi-Worker-Beispiele · retained Health/Capabilities · Metrics-Endpoint.
- **v1.0 — Production:** stabile MQTT-API + Command-Schema · stabiles Docker-Image · voller Test-Suite + CI · dokumentiertes Security-Modell · Deployment-Guide · Release-Artefakte.

---

## 18. Konkrete nächste Umsetzungsschritte (Migration Rust → Go)

Geordnet, klein, jeder Schritt lauffähig:

1. **Sprachentscheidung fixieren** (Abschnitt 1a). Wenn Go: Rust-Quellen (`src/*.rs`, `Cargo.*`) in einen `legacy/`-Branch/Tag sichern, dann aus `main` entfernen. `CONCEPT.md` §7/14/Anhang von Rust-Idiomen auf Go umschreiben.
2. **Go-Skeleton anlegen:** `go mod init`, Projektbaum aus §4, `cmd/trailtransfer` mit `run/validate-config/print-capabilities/version`.
3. **config + mqtt portieren:** die **guten** Teile aus dem Rust-Stand 1:1 übernehmen — Topic-Helper (`config.rs` §153–184), Reconnect-Loop, **LWT** (`mqtt_service.rs` §104). ENV-Präfix auf `TRAILTRANSFER_*` umstellen, `POLICY_FILE`/`RCLONE_CONFIG` ergänzen, SMB/SFTP-ENV **entfernen**.
4. **Command-Schema ersetzen:** altes `start/stop/options`-Modell raus, neues `copy/move/sync/check/cancel/status` mit `job_id` + `DisallowUnknownFields` rein.
5. **Policy-Engine + Job-Manager** neu bauen (gab es nie): `worker-policy.yaml`, `Evaluate()`, `job_id`-Dedupe, `max_parallel_jobs`-Semaphore, Cancel via `context`. **Das ist der eigentliche USP** gegenüber „cron + rclone".
6. **rclone-Runner umstellen:** Muster aus `rclone.rs` behalten (argv, `--use-json-log`-Parsing), aber **`rclone.conf` nicht mehr generieren** — Operator-bereitgestellt, RO gemountet; Remotes nur als Policy-Aliase.
7. **Kanonisches Hashing früh:** `command_hash`/`result_hash` sofort einbauen (auch wenn Evidence „offiziell" v0.3) — nachträglich teuer. snake_case-Result-Schema aus §9 statt des camelCase-JSON in `transfer.rs`.
8. **Docker reparieren:** neues `Dockerfile` (Go-Build + rclone-Runtime, non-root, Version-Injektion) und neues `docker-compose.yml` (worker + mosquitto + minio/sftp + `/config`-Mount). Der aktuelle Dockerfile ist kaputt (falscher Binary-Name, kein rclone).
9. **README neu schreiben** nach §12 — mit der Kernformel, nicht als „Uploader". Alten FileFlux-README ersetzen.
10. **CI konsolidieren:** `ci.yml` (gofmt/vet/lint/test/build) + `docker.yml` (GHCR, multi-arch, tag-getrieben); drei Alt-Workflows entfernen.
11. **Repo-Hygiene:** `NOTICE` (rclone-Attribution + Copyright), SPDX-Header, `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`; `docs/security-model.md` ausgründen.
12. **Beispiele committen:** `examples/commands/{copy,sync-dry-run,cancel,check}.json`, `worker-policy.yaml`, `rclone.conf.example`, `mosquitto.conf` — senkt die „erste 5 Minuten"-Hürde massiv.

**Kritischer Pfad für ein vorzeigbares v0.1:** Schritte 2 → 3 → 4 → 5 → 6 → 8 → 12. Alles andere (Evidence-Ausbau, Retries, Metriken) ist v0.2+.
```
