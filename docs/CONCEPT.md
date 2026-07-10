# TrailTransfer — Produkt- & Open-Source-Konzept

> **Claim:** MQTT-controlled rclone worker for audit-ready edge file transfers.
> **Kernformel:** MQTT command in → policy validation → rclone transfer → structured result/evidence out.
> **Leitsatz:** Works with any MQTT broker. Best with TrailMQ.

Status dieses Dokuments: Konzept / Strategie. Sprache: deutsche Erläuterungstexte, englische Ship-Copy (One-Liner, Pitches, README, Release Notes), weil diese Artefakte im Repo englisch ausgeliefert werden.

---

## 1. Executive Summary

TrailTransfer ist ein kleiner, fokussierter Open-Source **Edge Worker**, der Dateiübertragungen als **kontrollierte, nachweisbare Jobs** ausführt. Er nimmt Kommandos über **MQTT** entgegen, prüft jeden Job gegen eine **lokale Worker-Policy**, führt den eigentlichen Transfer über **rclone** aus und meldet den kompletten Lebenszyklus als **strukturierte Status-, Progress-, Result- und Evidence-Events** zurück.

TrailTransfer ersetzt rclone **nicht**. rclone bleibt die bewährte Transfer-Engine (SMB, SFTP, S3, MinIO, WebDAV, NAS, SharePoint u. v. m.). TrailTransfer ist die **MQTT-Control-Plane, Policy-Schicht, Job-Verwaltung und Evidence-Hülle** um rclone herum.

Der Kern des Wertversprechens ist nicht „Dateien schieben“ — das kann rclone allein. Der Kern ist: **jemand kann später sicher beweisen, welcher Job wann, mit welcher Policy, welche Dateien wohin übertragen hat — ohne dass das Edge-Gerät eine REST-API, SSH oder Shell-Automation exponiert.**

- **Standalone** mit jedem MQTT-Broker.
- **Native** Integration mit **TrailMQ** (Governance, Contracts, Evidence).
- **Self-hosted, edge-fähig, cloudfrei, domainlos.**
- **Kein** Remote-Shell-Executor. Nur allowlistete Transfer-Aktionen.
- Implementiert in **Rust** → ein kleines, statisches Binary im Distroless-/Scratch-Container, ideal für OT/Edge.
- Lizenz: **Apache-2.0** (bereits im Repo hinterlegt).

Positionierung im Ökosystem: **TrailSource** erzeugt kontextreiche Quell-Events, **TrailTransfer** bewegt Dateien als kontrollierte Jobs, **TrailMQ** regiert und beweist beides.

---

## 2. Product Story

### Ausgangsproblem
In Edge-/OT-/IIoT-Umgebungen entstehen Dateien lokal: Bilder, Reports, Logs, Messdatenexporte, Diagnosepakete, Prüfartefakte, Backups, CSV/JSON/XML. Sie müssen zuverlässig zu SMB, SFTP, S3, MinIO, WebDAV, NAS oder SharePoint. In der Praxis scheitert das an denselben Punkten:

- Zielsysteme können nicht aktiv vom Edge **ziehen**.
- Edge-Systeme sollen **keine eingehende REST-API** exponieren.
- **SSH-Zugriff** ist unpraktisch, breit und sicherheitspolitisch heikel.
- **Cron-/Shell-Skripte** sind schwer nachvollziehbar und driften auseinander.
- Es fehlt **zentrale Steuerung** über viele Geräte hinweg.
- **Fortschritt und Ergebnis** sind schlecht sichtbar.
- **Fehler sind schwer auditierbar.**
- Niemand weiß später **sicher**, welcher Job wann was übertragen hat.

### Die Lösung
TrailTransfer dreht die Richtung um: Der Edge-Worker **abonniert** ein Command-Topic und **veröffentlicht** seinen Zustand. Es gibt keinen offenen Port am Edge, nur eine ausgehende MQTT-Verbindung. Jeder Job wird gegen eine lokale Policy geprüft, von rclone ausgeführt und als sauberer Event-Lebenszyklus zurückgemeldet. Ergebnisse sind **audit-ready**: mit `command_hash`, `result_hash`, `policy_version` und `worker_version`.

### Ship-Copy (englisch)

**One-Liner**
> MQTT-controlled rclone worker for audit-ready edge file transfers.

**Elevator Pitch**
> TrailTransfer turns file transfers into governed, provable jobs. Edge machines receive transfer commands over MQTT, validate each one against a local policy, run the actual transfer through rclone, and report back structured status, progress, result and evidence events. No inbound HTTP API, no SSH, no brittle shell scripts — and a signed-off trail of exactly which job moved which files where, and when. Standalone with any MQTT broker; native with TrailMQ.

**README-Intro**
> # TrailTransfer
> **MQTT-controlled rclone worker for audit-ready edge file transfers.**
>
> TrailTransfer lets edge machines execute file transfer jobs without exposing HTTP APIs, SSH access or custom scripts. Jobs are received over MQTT, validated against a local worker policy, executed through rclone, and reported back as structured status, progress, result and evidence events.
>
> TrailTransfer does **not** replace rclone — rclone stays the transfer engine. TrailTransfer is the MQTT control plane, policy layer, job manager and evidence wrapper around it.
>
> _Works with any MQTT broker. Best with TrailMQ._

**GitHub Repository-Beschreibung (About, ≤ 350 Zeichen)**
> MQTT-controlled rclone worker for audit-ready edge file transfers. Receive transfer jobs over MQTT, validate against a local policy, run them through rclone, and get back structured status/progress/result/evidence events. Self-hosted, edge-ready, no cloud, no inbound API. Works with any broker; best with TrailMQ.

**LinkedIn-Kurztext**
> Most edge file transfers are invisible until they break. A cron job on some machine pushes reports to an SFTP server, and six months later nobody can prove what moved, when, or under which rules.
>
> TrailTransfer (Apache-2.0) fixes the control and evidence gap without adding a cloud: edge machines receive transfer jobs over MQTT, validate each against a local policy, run them through rclone, and report back structured result and evidence events — command hash, result hash, policy version. No inbound API, no SSH, no shell automation as a product surface.
>
> rclone stays the engine. TrailTransfer is the MQTT control plane, policy layer and evidence wrapper around it. Standalone with any broker, native with TrailMQ.

**Technische Produktbeschreibung**
> TrailTransfer is a single-purpose Rust worker. It maintains one outbound MQTT connection, subscribes to a per-worker command topic, and validates each incoming command against a declarative YAML policy (allowed actions, allowed source/target roots, size limits, delete guards, parallelism). Valid jobs are translated into a fixed rclone argument vector — never a shell string — and executed as a subprocess. rclone's `--use-json-log` / stats output is parsed into progress and result events. Secrets live only in `rclone.conf`, Docker secrets or environment; never in logs or events. Every job emits a canonical, hashable result event suitable for downstream evidence and audit.

### Abgrenzungen

**vs. rclone**
rclone ist eine hervorragende *Engine*, aber keine *Control Plane*. Es hat kein MQTT-Interface, kein zentrales Job-Modell über eine Flotte, keine Policy-Schicht, die riskante Aktionen erzwingt, und keine strukturierte Evidence-Ausgabe. TrailTransfer fügt genau diese Schicht hinzu und **ruft** rclone auf. „TrailTransfer = rclone + MQTT-Steuerung + Policy + Evidence.“

**vs. MFT-Systeme (Managed File Transfer)**
Klassische MFT-Suiten (GoAnywhere, MOVEit, Axway usw.) sind zentrale, serverlastige, oft lizenzteure, häufig cloud- oder web-zentrierte Plattformen. TrailTransfer ist bewusst das Gegenteil: **edge-first, event-driven, klein, self-hosted, ohne eingehende Angriffsfläche, ohne Lizenzkosten.** Es konkurriert nicht mit MFT-Governance-Featurebergen; es liefert die *nachweisbare Ausführung am Rand* — und überlässt Governance/Contracts bei Bedarf TrailMQ.

**vs. Shell-Skripte / Cronjobs**
Ein Cron+rclone-Skript ist unsichtbar, verteilt, undokumentiert und nach Monaten nicht mehr rekonstruierbar. TrailTransfer macht denselben Transfer **explizit, zentral steuerbar, policy-geschützt und nachweisbar** — mit einheitlichem Command-Schema, Live-Progress und hash-gesicherten Result-Events statt „hoffentlich lief der Cronjob durch“.

---

## 3. Positioning

TrailTransfer ist **nicht**:
- ✗ „better rclone“
- ✗ „file uploader“
- ✗ „enterprise MFT“
- ✗ „REST file transfer API“
- ✗ „generic command executor“

TrailTransfer **ist**:
- ✓ MQTT-controlled edge transfer worker
- ✓ policy-guarded rclone runner
- ✓ audit-ready job executor
- ✓ self-hosted edge file transfer component
- ✓ TrailMQ-native worker

**Kernformel**
```
TrailTransfer = MQTT command in
              → policy validation
              → rclone transfer
              → structured result / evidence out
```

Positionierungs-Disziplin: In README, Talks und Issues immer diese Formel verwenden. Sobald TrailTransfer als „Uploader“ oder „rclone-Wrapper“ beschrieben wird, verliert es seinen eigentlichen Wert (Steuerbarkeit + Nachweisbarkeit).

---

## 4. Target Users

| Zielgruppe | Problem | Nutzen | Typischer Use Case | Warum MQTT |
|---|---|---|---|---|
| **Edge-/OT-Engineers** | Geräte dürfen keine offenen Ports haben; Transfers sind heute Skript-Wildwuchs | Kontrollierte Transfers ohne Angriffsfläche, mit Policy-Grenzen | Prüfartefakte/Diagnosepakete von einer Maschine ins Firmen-NAS/SFTP | Ausgehende Verbindung, kein Inbound-Port; passt zu segmentierten OT-Netzen |
| **IIoT-Integratoren** | Viele heterogene Geräte, einheitliche Steuerung fehlt | Ein Command-Schema für eine ganze Flotte | Zentrales Auslösen von Report-Uploads über 200 Linien | Pub/Sub skaliert über Flotten; ein Broker steuert viele Worker |
| **Automatisierungsteams** | Übergaben zwischen Systemen sind fragil und intransparent | Event-getriggerte, quittierte Transfers | „Batch fertig“-Event → Transfer-Job → Result-Event zurück in den Workflow | Event-driven statt Polling; natürliche Verkettung mit anderen Events |
| **Betreiber von Legacy-Speicherzielen** | SMB/SFTP/NAS können nicht selbst ziehen, keine moderne API | Legacy-Ziele werden Teil einer modernen Event-Pipeline | Nächtlicher Push von CSV/XML in eine SMB-Freigabe | Broker als moderne Fassade vor altem Storage |
| **Homelab / Self-hosted** | Verstreute Cronjobs, kein Überblick, keine Cloud gewünscht | Ein sauberer, selbst gehosteter Worker mit Live-Status | Kamera-Snapshots/Backups nach MinIO/NAS schieben | Ein Broker (oft schon vorhanden) steuert alles; keine Cloud |
| **MQTT-/Event-driven-Entwickler** | Dateitransfer bricht aus dem Event-Modell aus | Datei-Jobs werden zu erststrangigen Events | Transfer als Teil einer größeren Event-Choreografie | Bleibt im gewohnten Pub/Sub-Modell; keine Fremdparadigmen |
| **TrailMQ-Nutzer** | Wollen Systeme registrieren, Rechte und Evidence verwalten | TrailTransfer ist ein „erwartetes System“ mit Contracts & Evidence | Governter Transfer-Worker in der TrailMQ-Timeline | Native Topic-Struktur, Rechte- und Evidence-Modell passt 1:1 |
| **rclone-Nutzer, die zentraler steuern wollen** | rclone läuft, aber verstreut und ohne Nachvollziehbarkeit | Behalten rclone, gewinnen Steuerung + Audit | Bestehende rclone-Remotes zentral per MQTT auslösen | Zentrale Steuerung ohne rclone aufzugeben |

---

## 5. Open Source Strategy

**Warum Open Source hier zwingend ist**
TrailTransfer bewegt Daten in regulierten, sicherheitskritischen Umgebungen (OT, Industrie, teils GxP-nah). Genau dort ist **Vertrauen = Transparenz**. Ein Tool, das entscheidet, welche Datei wohin darf, und das Evidence erzeugt, muss **prüfbar** sein. Closed Source wäre in diesem Kontext ein Widerspruch zum eigenen Wertversprechen (Auditierbarkeit).

**Warum Edge-/OT-Nutzer Transparenz brauchen**
Sie müssen den Code auditieren, in Air-Gapped-Netzen betreiben, selbst bauen und langfristig warten dürfen — unabhängig vom Fortbestand eines Anbieters. Open Source ist hier Betriebssicherheit, nicht Ideologie.

**Warum self-hosted / keine Cloud**
OT-Netze sind segmentiert und oft offline. Eine Cloud-Abhängigkeit würde TrailTransfer für den Kernmarkt disqualifizieren. Alles läuft lokal: Worker, Broker, Ziele. Keine Telemetrie nach außen, keine Pflicht-Accounts.

**Warum rclone als Engine**
rclone ist erprobt, breit unterstützt (40+ Backends), aktiv gepflegt und MIT-lizenziert. Es selbst nachzubauen wäre Verschwendung und ein Sicherheitsrisiko. TrailTransfer profitiert von rclones Reife und bleibt selbst klein.

**Warum klein und fokussiert**
Der Wert liegt in Schärfe: MQTT-Steuerung + Policy + Evidence. Feature-Wildwuchs (eigene Backends, Web-UI, Scheduler, generischer Command-Runner) würde die Angriffsfläche vergrößern und die Positionierung verwässern. Regel: **Alles, was nicht Steuerung, Policy oder Nachweis ist, gehört nicht in den Kern.**

**Verhältnis zu TrailMQ**
TrailTransfer funktioniert mit **jedem** MQTT-Broker (Mosquitto, HiveMQ, EMQX …). Mit TrailMQ wird es *besser*: Registrierung als erwartetes System, Pub/Sub-Rechte, Command-Contracts, Result-Auditing, Evidence Reports. Aber: **keine harte Abhängigkeit.** TrailTransfer darf ohne TrailMQ nie brechen.

> **Leitsatz:** Works with any MQTT broker. Best with TrailMQ.

---

## 6. License Recommendation

> ⚠️ **Keine Rechtsberatung.** Diese Bewertung ist eine technische/strategische Einordnung. Die endgültige Lizenzentscheidung liegt beim Maintainer und ggf. einer juristischen Prüfung.

### Bewertungsmatrix

| Kriterium | MIT | **Apache-2.0** | MPL-2.0 | GPL-3.0 | AGPL-3.0 |
|---|---|---|---|---|---|
| OSS-Klarheit | Hoch | **Hoch** | Mittel | Mittel | Mittel |
| Enterprise-/Industrie-Adoption | Hoch | **Sehr hoch** | Hoch | Niedrig | Sehr niedrig |
| Schutz vor proprietärer Vereinnahmung | Niedrig | Niedrig | Mittel (file-level) | Hoch | Sehr hoch (Netz-Copyleft) |
| Patent-Schutz (expliziter Grant) | ✗ | **✓** | ✓ | ✓ | ✓ |
| Kompatibilität mit rclone (MIT, Subprozess) | ✓ | **✓** | ✓ | ✓ | ✓ |
| Contributor-Freundlichkeit | Hoch | **Hoch** | Mittel | Mittel | Mittel |
| Rechtliche Verständlichkeit | Sehr hoch | **Hoch** | Mittel | Mittel | Niedrig |
| Wirkung in OT/Industrie/regulierten Umfeldern | Gut | **Sehr gut** | Gut | Problematisch | Oft geblockt |
| Spätere kommerzielle Services (Consulting, Support) | Gut | **Sehr gut** | Gut | Gut | Eingeschränkt (Adoption ↓) |

### Empfehlung: **Apache-2.0** ✅ (bereits im Repo hinterlegt)

Apache-2.0 ist für TrailTransfer die richtige Wahl, weil die erklärte Priorität **breite, angstfreie Adoption in Industrieumgebungen** ist:

1. **Permissiv** — Firmen dürfen es einbetten, verändern, ausrollen, ohne Copyleft-Verpflichtungen. Genau das braucht ein Edge-Baustein, der in fremde Deployments wandert.
2. **Expliziter Patent Grant (§3)** — Der entscheidende Vorteil gegenüber MIT. In Industrie/OT ist Patent-Risiko ein reales Beschaffungsthema; der ausdrückliche Grant + Defensive-Termination senkt Adoptionshürden bei Rechtsabteilungen.
3. **Enterprise-Standard** — Apache-2.0 steht auf jeder „erlaubt“-Liste großer Firmen. Kein Klärungsbedarf, kein Ticket bei Legal.
4. **rclone-kompatibel** — rclone ist MIT und wird als **Subprozess** aufgerufen (kein Linking) → ohnehin keine Lizenz-Kontamination, unabhängig von der eigenen Lizenz. Apache-2.0 ist zusätzlich sauber MIT-kompatibel.
5. **Contributor- und servicefreundlich** — klare Contribution-Terms (§5), NOTICE-Datei für Attribution, gute Basis für spätere kommerzielle Services (Support/Hardening/Integration) ohne Lizenzkonflikte.

**Praktische To-dos bei Apache-2.0:** `LICENSE` (vorhanden), `NOTICE`-Datei mit Copyright-Zeile ergänzen, kurzer SPDX-Header (`// SPDX-License-Identifier: Apache-2.0`) in Quellcode-Dateien, klarstellen dass rclone separat MIT-lizenziert und **nicht** Teil des Distributionsartefakts ist (nur zur Laufzeit aufgerufen).

### AGPL-3.0 — bewusst *nicht*, aber sauber diskutiert

AGPL wäre die Wahl, wenn das Hauptrisiko wäre, dass ein **Cloud-/SaaS-Anbieter TrailTransfer zu einem gehosteten Dienst forkt**, ohne zurückzugeben. AGPLs Netz-Copyleft würde einen solchen Fork zwingen, seine Änderungen offenzulegen.

**Warum das hier nicht passt:**
- TrailTransfer ist **edge-/self-hosted**, kein SaaS-Kandidat. Das SaaS-Fork-Szenario, gegen das AGPL schützt, ist für ein Air-Gapped-Edge-Tool schlicht wenig relevant.
- AGPL wird in vielen **Industrie- und OT-Beschaffungsprozessen pauschal geblockt** (verbreitete „No-AGPL“-Policies). Das kollidiert frontal mit dem Ziel „ohne Lizenzangst in Industrieumgebungen nutzbar“.
- Die **Abschreckung** (verlorene Adoption) übersteigt hier den **Schutznutzen** (unwahrscheinlicher SaaS-Fork).

**Fazit:** AGPL tauscht genau die Breite ein, die TrailTransfer gewinnen will. → **Apache-2.0 bleibt die Empfehlung.** (MPL-2.0 wäre der vernünftigste Kompromiss, falls später doch etwas Datei-Level-Copyleft-Schutz gewünscht wird — ohne die AGPL-Adoptionsstrafe.)

---

## 7. Architecture

### ASCII-Übersicht
```
                 ┌──────────────────────────────┐
                 │   MQTT Broker / TrailMQ       │
                 │  (auth, ACL, contracts, LWT)  │
                 └──────────────┬───────────────┘
                                │  command  (trailtransfer/{worker_id}/commands)
                                ▼
                 ┌──────────────────────────────┐
                 │     TrailTransfer Worker      │
                 │  ┌────────────────────────┐  │
                 │  │ 1. schema validation   │  │
                 │  │ 2. policy validation   │  │  ◄── worker-policy.yaml
                 │  │ 3. idempotency (job_id)│  │
                 │  │ 4. build rclone argv   │  │  (no shell string!)
                 │  └───────────┬────────────┘  │
                 └──────────────┼───────────────┘
                                │  exec (arg vector)
                                ▼
                 ┌──────────────────────────────┐
                 │            rclone             │  ◄── rclone.conf (secrets)
                 │      (the transfer engine)    │
                 └──────────────┬───────────────┘
                                │
             ┌──────────┬───────┴────┬──────────┬──────────┐
             ▼          ▼            ▼          ▼          ▼
           SMB        SFTP        S3/MinIO    WebDAV    NAS / SharePoint
             ▲          ▲            ▲          ▲          ▲
             └──────────┴─────┬──────┴──────────┴──────────┘
                              │  status / progress / result / evidence
                              ▼
                 ┌──────────────────────────────┐
                 │   MQTT Broker / TrailMQ       │
                 │  timeline · audit · evidence  │
                 └──────────────────────────────┘
```

### Interne Komponenten (Rust)
- **MQTT client** (`rumqttc`): eine ausgehende Verbindung, Auto-Reconnect, QoS-1 für Commands/Results, **retained** Health/Capabilities, **LWT** (Last Will) für „worker offline“.
- **Command decoder + schema validator**: JSON → typisierte Struct (`serde`), strikte Schema-Prüfung, unbekannte Felder ablehnen.
- **Policy engine**: lädt `worker-policy.yaml`, prüft Action/Source/Target/Size/Delete/Parallelität; liefert eine Ablehnung als sauberes `rejected`-Result-Event (nie stiller Fehlschlag).
- **Job manager**: `job_id`-Idempotenz, Parallelitäts-Semaphore, Cancel-Handling, Zustandsmaschine `received → validated → running → completed | failed | rejected | cancelled`.
- **rclone runner**: baut ein **Argument-Vektor** (kein Shell-String), startet Subprozess, parst `--use-json-log`/`--stats`-Ausgabe für Progress; kapselt Exit-Code.
- **Event publisher**: kanonische JSON-Events, Hashing (command/result), Publish auf die jeweiligen Topics.
- **Evidence builder**: kanonisiert Result → `result_hash`, hängt `policy_version`, `worker_version`, `command_hash` an.

---

## 8. MQTT API

### Topic-Struktur
**Subscribe (Worker hört zu):**
```
trailtransfer/{worker_id}/commands
```

**Publish (Worker meldet):**
```
trailtransfer/{worker_id}/health                       (retained, + LWT)
trailtransfer/{worker_id}/capabilities                 (retained)
trailtransfer/{worker_id}/jobs/{job_id}/status
trailtransfer/{worker_id}/jobs/{job_id}/progress
trailtransfer/{worker_id}/jobs/{job_id}/result
trailtransfer/{worker_id}/jobs/{job_id}/logs
```

### QoS / Retention / LWT — Designregeln
- **commands**: QoS 1 (at-least-once). Idempotenz über `job_id` fängt Duplikate ab.
- **status/result**: QoS 1. Result ist die verlässliche Quittung.
- **progress/logs**: QoS 0 (Volumen, verlustbar).
- **health/capabilities**: QoS 1, **retained** → neue Subscriber kennen den Zustand sofort.
- **LWT**: retained Health-Topic wird bei Verbindungsverlust auf `{"status":"offline"}` gesetzt → Broker/TrailMQ erkennt tote Worker.

### Command-/Event-Schema

**copy command**
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

**status event** (Verlauf)
```json
{
  "event_type": "status",
  "job_id": "job-001",
  "worker_id": "worker-01",
  "state": "running",
  "timestamp": "2026-01-01T10:00:03Z"
}
```

**progress event**
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

**result event** (audit-ready)
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

**cancel command**
```json
{ "job_id": "job-001", "action": "cancel" }
```

**Fehler-/Ablehnungs-Result** (Policy-Verstoß → nie stiller Fehlschlag)
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

---

## 9. Security Model

**Grundsatz: TrailTransfer ist KEIN Remote-Shell-Executor.** Es führt niemals beliebige Befehle aus. Es führt *allowlistete Transfer-Aktionen* aus, deren Parameter gegen eine Policy geprüft sind.

- **Nur erlaubte Actions** — `copy | move | sync | check` (allowlist). Keine `delete`/`purge` ohne explizite Policy-Freigabe.
- **Source/Target gegen Policy** — jeder Pfad/Remote muss unter `allowed_sources` / `allowed_targets` fallen. Absolute Ziele optional verboten (`allow_absolute_targets: false`).
- **Keine beliebigen Shell-Commands** — Kommandos sind ein festes Schema, kein Freitextfeld. rclone wird über einen **Argument-Vektor** (`Command::new("rclone").args([...])`) gestartet, **nie** über eine interpolierte Shell-Zeile → keine Command-Injection.
- **rclone sicher gekapselt** — feste Flags, gewhitelistete Aktions-Templates; kein Durchreichen beliebiger rclone-Flags aus dem Command.
- **Secrets** — ausschließlich über `rclone.conf`, Docker Secrets oder ENV. Nie im Command, nie im Event, nie im Log.
- **Keine Secrets in Logs** — Redaction/Filter; rclone mit `--log-level` ohne Credential-Ausgabe; Ziel-Remotes werden als Alias (`sftp-demo:`) geloggt, nicht mit Zugangsdaten.
- **Gefährliche Operationen off by default** — `sync` mit Löschen der Zielseite (`allow_sync_delete`) und `delete` (`allow_delete`) sind standardmäßig **aus**.
- **dry-run default möglich** — `dry_run_default: true` für Setup/Onboarding, damit erste Jobs nichts verändern.
- **max file size** — `max_file_size_mb` verhindert Runaway-Transfers.
- **allowed sources / allowed targets** — harte Grenzen des Bewegungsraums.
- **max parallel jobs** — Ressourcen- und Blast-Radius-Grenze.
- **command schema validation** — strikt, unbekannte Felder → Ablehnung.
- **job_id idempotency** — wiederholte `job_id` wird nicht doppelt ausgeführt; schützt bei QoS-1-Redelivery.

**Threat-Model-Kurzform:** Angriffsfläche = *eingehende* MQTT-Commands. Verteidigung = Broker-Auth/ACL (bzw. TrailMQ-Rechte) + strikte Schema-Validierung + Policy-Engine + kein Shell-Eval + kein offener Inbound-Port am Edge.

---

## 10. Policy Model

`examples/worker-policy.yaml`:
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

allow_delete: false
allow_sync_delete: false
allow_absolute_targets: false
dry_run_default: true
require_job_id: true
```

**Operator-freundliche Erklärung:**
- `allowed_actions` — was dieser Worker überhaupt darf. Alles nicht Gelistete wird abgelehnt.
- `allowed_sources` — nur aus diesen Ordnern darf gelesen werden. Ein Command mit `source: /etc` wird abgelehnt.
- `allowed_targets` — nur zu diesen (rclone-)Zielen darf geschrieben werden. Aliase (`sftp-demo:`) verweisen auf `rclone.conf`, wo die Zugangsdaten liegen.
- `max_file_size_mb` — Schutz vor riesigen Einzeldateien / Fehlkonfiguration.
- `max_parallel_jobs` — wie viele Jobs gleichzeitig laufen dürfen (Ressourcen + Blast Radius).
- `allow_delete` — ob Löschaktionen erlaubt sind. **Default aus.**
- `allow_sync_delete` — ob `sync` auf der Zielseite löschen darf. **Default aus** (verhindert versehentliches Leeren des Ziels).
- `allow_absolute_targets` — ob absolute Zielpfade erlaubt sind. **Default aus.**
- `dry_run_default` — wenn `true`, laufen Jobs ohne `dry_run`-Feld standardmäßig als Trockenlauf. Ideal fürs Onboarding.
- `require_job_id` — erzwingt `job_id` in jedem Command (Idempotenz + Nachvollziehbarkeit).

**Betriebsempfehlung:** Policy in Versionskontrolle halten, `policy_version` bei jeder Änderung hochziehen (erscheint in jedem Result-Event → Änderungen sind später zuordenbar).

---

## 11. Evidence Model

Ziel: aus jedem Job ein **nachweisbares, manipulationssensitives** Artefakt machen — ohne externe Infrastruktur.

- **command_hash** — SHA-256 über das **kanonische** empfangene Command (sortierte Keys, kein Whitespace). Beweist, *welche* Anweisung ausgeführt wurde.
- **result_hash** — SHA-256 über das kanonische Result-Objekt **ohne** das `result_hash`-Feld selbst. Beweist die *Integrität* des Ergebnisses.
- **policy_version** — welche Regelbasis galt.
- **worker_version** — welche Software-Version lief.
- **manifest** (ab v0.3) — optionale Liste der übertragenen Dateien mit Größe und Hash → „genau diese Dateien wurden bewegt“.
- **Kanonisierung** ist Pflicht: gleiche Semantik ⇒ gleicher Hash. Ohne kanonische Serialisierung sind Hashes wertlos.
- **Optionale Signatur** (später) — Worker kann Result-Events signieren (Ed25519), sodass Empfänger Herkunft verifizieren.

**Chain of custody:** `command_hash` (was wurde befohlen) → Policy-Entscheidung (durfte es) → rclone-Ausführung (`rclone_exit_code`, Zähler) → `result_hash` (was kam heraus). TrailMQ kann diese Kette als Evidence Report bündeln.

---

## 12. TrailMQ Integration

TrailTransfer ist als **idealer TrailMQ-Participant** gebaut.

**TrailTransfer executes controlled file jobs. TrailMQ governs and proves them.**

Worker **subscribes**: `trailtransfer/{worker_id}/commands`
Worker **publishes**: `health`, `capabilities`, `jobs/{job_id}/status|progress|result|logs`

TrailMQ kann darauf:
- Worker als **erwartetes System** registrieren (bekannte Identität, bekannte Topics).
- **Pub/Sub-Rechte** verwalten (wer darf Commands senden, wer Results lesen).
- **Command-Topics kontrollieren** (nur berechtigte Sender).
- **Result-Topics auditieren** (jedes Ergebnis wird erfasst).
- Transfer-Jobs in der **Timeline** anzeigen.
- **Evidence Reports** erzeugen (command/result-Hashes, Policy-Version, Zeitachse).
- **Job-Result-Events als Evidence** aufnehmen.

**Contract-Beispiel (konzeptionell):** TrailMQ definiert für `trailtransfer/*/commands` ein erwartetes Schema (action ∈ allowlist, job_id required) und für `.../result` ein Evidence-Schema (Hashes + Versionen Pflicht). Verstöße sind sichtbar, nicht still.

Wichtig bleibt: **kein harter Zwang.** Ohne TrailMQ läuft alles gegen jeden Broker; mit TrailMQ kommen Governance, Rechte, Contracts und Evidence obendrauf.

### Ökosystem-Narrativ
```
TrailSource    → emits context-rich source events
TrailTransfer  → moves files as controlled jobs
TrailMQ        → governs & proves both (systems · contracts · audit · evidence)
```
> **The Trail projects make edge communication observable, controllable and auditable.**
> **Die Trail-Projekte machen Edge-Kommunikation sichtbar, steuerbar und nachweisbar.**

---

## 13. Roadmap

**v0.1 — Walking skeleton**
- MQTT connect/reconnect, `worker_id`, command topic
- `copy` action, rclone subprocess
- policy validation, status/result events
- Docker Compose Demo, README + examples
- Apache-2.0 license

**v0.2 — Robust jobs**
- job queue, `cancel`
- progress parsing, retry
- `capabilities` event, structured errors

**v0.3 — Evidence**
- evidence hashes (command/result), manifest
- policy versioning
- TrailMQ example contracts

**v0.4 — Observability & scale**
- optional rclone `rc` API
- multi-worker examples
- better observability, retained health/capabilities

**v1.0 — Production**
- stable MQTT API + command schema
- stable Docker image
- tests + CI, security model documented
- production deployment guide

---

## 14. Repository Structure

```
README.md
LICENSE                         # Apache-2.0 (vorhanden)
NOTICE                          # Copyright + rclone-Attribution
CONTRIBUTING.md
CODE_OF_CONDUCT.md
SECURITY.md
CHANGELOG.md
Dockerfile
docker-compose.yml
.env.example
.github/workflows/ci.yml
.github/workflows/docker.yml

examples/
  commands/
    copy.json
    sync-dry-run.json
    cancel.json
  worker-policy.yaml
  rclone.conf.example
  mosquitto.conf
  sample-files/

docs/
  architecture.md
  mqtt-api.md
  command-schema.md
  worker-policy.md
  evidence-model.md
  security-model.md
  trailmq-integration.md
  development.md
  CONCEPT.md                    # dieses Dokument
```
> Migrationshinweis: Der bestehende SMB/SFTP-spezifische Code (`upload.rs`, `smbclient`) wird hinter rclone-Remotes zusammengeführt; `config.rs` referenziert bereits `rclone_bin` und `worker_id`. Alt-Workflows `docker-image.yml`/`docker-publish.yml` auf die neuen `ci.yml`/`docker.yml` konsolidieren.

---

## 15. README Draft (Struktur + Einstieg)

Reihenfolge: **Header → Claim → Badges → Quick Summary → Problem → Why MQTT? → Why rclone? → Architecture → Quickstart → Example Command → Example Result Event → Worker Policy → MQTT Topics → Security Model → TrailMQ Integration → Roadmap → License → Contributing.**

```markdown
# TrailTransfer

**MQTT-controlled rclone worker for audit-ready edge file transfers.**

[![License: Apache 2.0](badge)](LICENSE) [![CI](badge)](actions) [![Docker](badge)](ghcr)

TrailTransfer lets edge machines execute file transfer jobs without exposing
HTTP APIs, SSH access or custom scripts. Jobs are received over MQTT, validated
against a local worker policy, executed through rclone, and reported back as
structured status, progress, result and evidence events.

TrailTransfer does **not** replace rclone — rclone stays the transfer engine.
TrailTransfer is the MQTT control plane, policy layer, job manager and evidence
wrapper around it.

> Works with any MQTT broker. Best with TrailMQ.

## Why MQTT?
No inbound port on the edge, one outbound connection, central control across a
fleet, event-driven chaining, and live status/result feedback.

## Why rclone?
Proven, actively maintained, 40+ storage backends (SMB, SFTP, S3, MinIO,
WebDAV, NAS, SharePoint …). We don't reinvent transfers — we govern them.

## Quickstart
docker compose up  →  publish examples/commands/copy.json  →  watch result event
```

---

## 16. GitHub Project Setup

**Repository description / About:**
> MQTT-controlled rclone worker for audit-ready edge file transfers. Self-hosted, edge-ready, no cloud, no inbound API. Works with any broker; best with TrailMQ.

**Topics/Tags:**
`mqtt` · `rclone` · `edge` · `file-transfer` · `iiot` · `self-hosted` · `sftp` · `smb` · `minio` · `docker` · `audit` · `event-driven` · `ot` · `rust`

**Pinned repo one-liner:**
> Governed, audit-ready edge file transfers over MQTT — powered by rclone.

**First Release notes — v0.1.0:**
```markdown
## TrailTransfer v0.1.0 — first walking skeleton

MQTT-controlled rclone worker for audit-ready edge file transfers.

### What works
- Outbound MQTT connect with auto-reconnect (`worker_id`-scoped topics)
- `copy` action executed through rclone as a subprocess
- Local YAML worker policy: allowed actions / sources / targets, size limit,
  parallelism, delete guards, dry-run default
- Structured `status` and `result` events (incl. command_hash / result_hash,
  policy_version, worker_version)
- Docker Compose demo (worker + Mosquitto + SFTP target) and example commands

### Not yet
- job queue, cancel, live progress, retry (→ v0.2)
- manifest & signed evidence (→ v0.3)

### Notes
- rclone stays the engine and is called as a subprocess (MIT, not bundled).
- Works with any MQTT broker; native governance with TrailMQ.

Licensed under Apache-2.0.
```

**Issue labels:** `type: bug`, `type: feature`, `type: docs`, `type: security`, `area: mqtt`, `area: rclone`, `area: policy`, `area: evidence`, `area: ci`, `good first issue`, `help wanted`, `priority: high`, `discussion`.

**Good first issues (Beispiele):**
- Add `sync-dry-run.json` example + doc walkthrough.
- Add SPDX headers to all `.rs` files.
- Redact remote credentials in log output (unit test).
- Validate `job_id` presence when `require_job_id: true`.
- Document each policy field in `docs/worker-policy.md`.
- Add a `check` action example against MinIO.

**Contribution guidelines summary:** Fork → Branch → `cargo fmt` + `cargo clippy -D warnings` + `cargo test` → conventional commit → PR with description + linked issue. DCO/`Signed-off-by` empfohlen. Security-Meldungen privat via `SECURITY.md`, nicht als öffentliches Issue.

---

## 17. Monetization without Open-Core

**Prinzip: Der Code bleibt vollständig Open Source. Kein Feature hinter Paywall. Geld kommt über Expertise, Vertrauen und Integration — nicht über Gating.**

- **Consulting** — Architektur für Edge-Transfer-Flotten.
- **Integration** — TrailTransfer in bestehende OT-/IIoT-Landschaften einbinden.
- **TrailMQ-Anbindung** — Governance/Contracts/Evidence produktiv aufsetzen.
- **Enterprise Support** — SLAs, priorisierte Fixes, Upgrade-Begleitung.
- **Security Review** — Threat Modeling, Härtung des Deployments.
- **OT Hardening** — Netzsegmentierung, Broker-ACLs, Secret-Handling.
- **Custom Connectors / rclone-Profile** — kundenspezifische Ziele/Setups.
- **Validated deployment templates** — geprüfte Compose/Helm-Vorlagen.
- **Schulungen** — Betrieb, Policy-Design, Evidence-Auswertung.
- **Managed deployment (optional)** — Betrieb im Kundenauftrag.
- **GMP-/Audit-Dokumentationspakete (optional, außerhalb des Codes)** — Qualifizierungs-/Validierungsunterlagen für regulierte Umgebungen.

Faustregel: **Was allen nützt → in den Open-Source-Kern. Was einzelnen Kunden Zeit/Risiko spart → bezahlte Dienstleistung.**

---

## 18. Risks

| Risiko | Wirkung | Gegenmaßnahme |
|---|---|---|
| **Scope Creep** (Web-UI, Scheduler, generischer Command-Runner) | Verwässert Positionierung, vergrößert Angriffsfläche | Harte Kern-Definition: nur Steuerung/Policy/Evidence; alles andere ablehnen oder als separates Projekt |
| **„Remote-Shell“-Wahrnehmung** | Sicherheits-Blocker in OT-Beschaffung | Security-Modell prominent dokumentieren; kein Shell-Eval; Argument-Vektor; allowlist-only |
| **rclone-Kopplung / Versions-Drift** | Verhalten ändert sich mit rclone-Version | rclone-Version im `capabilities`/`result`-Event ausweisen; getestete Version pinnen; Kompatibilität dokumentieren |
| **Command-Injection über Felder** | Kritische Sicherheitslücke | Strikte Schema-Validierung, keine Freitext-Flags, Pfad-Normalisierung gegen Policy |
| **Secret-Leak in Logs/Events** | Compliance-Verstoß | Redaction, Remote-Aliase statt Credentials, Tests |
| **Evidence ohne Kanonisierung** | Hashes wertlos, „Fake Audit“ | Kanonische JSON-Serialisierung erzwingen; Hash-Tests |
| **TrailMQ-Abhängigkeit schleicht sich ein** | Bricht Standalone-Versprechen | CI-Test gegen Vanilla-Mosquitto; TrailMQ nur additiv |
| **Zu breite Ambition / zu wenig Fokus** | Projekt bleibt unfertig | Roadmap diszipliniert; v0.1 klein und lauffähig |
| **Namens-/Marken-Kollision** | Rechtliche/SEO-Probleme | Namen prüfen; „Trail“-Ökosystem konsistent halten |

---

## 19. Recommended Next Steps

1. **Repo-Rename final ziehen** — Historie/README/Badges von „FileFlux/super-fast-smb-image-uploader“ vollständig auf TrailTransfer umstellen (Cargo.toml ist bereits `trailtransfer`).
2. **`NOTICE` + SPDX-Header** ergänzen (Apache-2.0-Hygiene), rclone-Attribution klarstellen.
3. **v0.1 fertigstellen** — genau der Roadmap-v0.1-Scope; nicht mehr. `copy` + Policy + status/result + Compose-Demo.
4. **Security-Modell als eigenes `docs/security-model.md`** — der wichtigste Vertrauensbaustein für OT.
5. **Policy-Engine + Argument-Vektor-Runner** priorisieren (nicht Shell-Interpolation) — das ist der eigentliche USP gegenüber „cron + rclone“.
6. **Kanonisches Hashing** früh einbauen, auch wenn Evidence erst v0.3 „offiziell“ ist — nachträglich ist es Aufwand.
7. **CI konsolidieren** — `ci.yml` (fmt/clippy/test/build) + `docker.yml` (GHCR, multi-arch, Tag-getrieben) statt drei Alt-Workflows.
8. **TrailMQ-Contract-Beispiel** als Show-case vorbereiten (v0.3), um das Ökosystem-Narrativ greifbar zu machen.
9. **README nach obiger Struktur** neu schreiben — mit der Kernformel oben, nicht als „Uploader“.
10. **Drei Beispiel-Commands + Beispiel-Result** ins Repo (`examples/`) — das senkt die „erste 5 Minuten“-Hürde massiv.

---

## Anhang: CI/CD & Docker

**`ci.yml` (on push / PR):** `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test` → `cargo build --release`. Optional: `cargo audit`.

**`docker.yml` (on tag `v*`):** `docker buildx` multi-arch (`linux/amd64`, `linux/arm64`, optional `linux/arm/v7` für Edge/RPi) → Push nach **GHCR** mit Tags `vX.Y.Z` **und** `latest` → nur `GITHUB_TOKEN`, **keine** Langzeit-Secrets. SBOM/Provenance optional (`--sbom=true --provenance=true`).

**Image:** Multi-stage — Rust-Builder (musl, statisch) → **distroless/scratch**-Runtime + installiertes `rclone`-Binary. Klein, edge-tauglich, minimale Angriffsfläche. Version über Build-Arg in `worker_version` einbrennen (erscheint in jedem Result-Event).
