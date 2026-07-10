// Command trailtransfer is an MQTT-controlled, policy-guarded rclone worker for
// audit-ready edge file transfers.
//
// Usage:
//
//	trailtransfer run [--config path]
//	trailtransfer validate-config [--config path]
//	trailtransfer print-capabilities [--config path]
//	trailtransfer version
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/RainerGewalt/trailtransfer/internal/config"
	"github.com/RainerGewalt/trailtransfer/internal/events"
	"github.com/RainerGewalt/trailtransfer/internal/evidence"
	"github.com/RainerGewalt/trailtransfer/internal/health"
	"github.com/RainerGewalt/trailtransfer/internal/jobs"
	"github.com/RainerGewalt/trailtransfer/internal/logging"
	"github.com/RainerGewalt/trailtransfer/internal/mqtt"
	"github.com/RainerGewalt/trailtransfer/internal/policy"
	"github.com/RainerGewalt/trailtransfer/internal/rclone"
	"github.com/RainerGewalt/trailtransfer/internal/version"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	switch os.Args[1] {
	case "run":
		cmdRun(os.Args[2:])
	case "validate-config":
		cmdValidateConfig(os.Args[2:])
	case "print-capabilities":
		cmdPrintCapabilities(os.Args[2:])
	case "verify":
		cmdVerify(os.Args[2:])
	case "version":
		fmt.Println(version.String())
	case "-h", "--help", "help":
		usage()
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n\n", os.Args[1])
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprint(os.Stderr, `trailtransfer — MQTT-controlled rclone worker for audit-ready edge file transfers

Commands:
  run                 start the worker
  validate-config     load config + policy, report validity, exit 0/1
  print-capabilities  print the capabilities event as JSON
  verify <journal>    independently verify an evidence journal, exit 0/1
  version             print the worker version

Flags (run/validate-config/print-capabilities):
  --config <path>     path to a YAML config file (optional; TRAILTRANSFER_* env overrides)
`)
}

func cmdRun(args []string) {
	cfgPath := parseConfigFlag("run", args)
	cfg, pol, err := loadAll(cfgPath)
	if err != nil {
		fatal(err)
	}
	log := logging.Setup(cfg.LogLevel)
	log.Info("starting TrailTransfer", "worker_id", cfg.WorkerID, "version", version.String(), "policy_version", pol.PolicyVersion)

	runner := &rclone.Runner{
		RclonePath:   cfg.RcloneBinary,
		RcloneConfig: cfg.RcloneConfig,
		MaxSizeMB:    pol.MaxFileSizeMB,
	}
	if !rclone.Available(cfg.RcloneBinary) {
		log.Warn("rclone binary not found — transfer jobs will fail until it is on PATH", "binary", cfg.RcloneBinary)
	}

	sealer, err := evidence.NewSealer(cfg.WorkerID, cfg.EvidenceJournal)
	if err != nil {
		fatal(fmt.Errorf("evidence journal: %w", err))
	}
	defer sealer.Close()
	if sealer.JournalEnabled() {
		log.Info("evidence journal enabled", "path", cfg.EvidenceJournal)
	} else {
		log.Warn("evidence journal disabled — hash chain is in-memory only; set TRAILTRANSFER_EVIDENCE_JOURNAL for durable, recoverable evidence")
	}

	lwt, _ := json.Marshal(events.HealthEvent{
		EventType: events.TypeHealth,
		Status:    "offline",
		WorkerID:  cfg.WorkerID,
		Version:   version.String(),
	})

	client := mqtt.New(mqtt.Options{
		Broker:     cfg.BrokerURL(),
		ClientID:   "trailtransfer-" + cfg.WorkerID,
		Username:   cfg.MQTT.Username,
		Password:   cfg.MQTT.Password,
		KeepAlive:  time.Duration(cfg.MQTT.KeepAliveSeconds) * time.Second,
		LWTTopic:   cfg.HealthTopic(),
		LWTPayload: lwt,
	})

	pub := events.NewPublisher(client, cfg, version.String(), sealer)
	mgr := jobs.NewManager(pol, pub, runner, log)

	caps := events.CapabilitiesEvent{
		SupportedActions:      mgr.SupportedActions(),
		MaxParallelJobs:       pol.MaxParallelJobs,
		RcloneAvailable:       rclone.Available(cfg.RcloneBinary),
		EvidenceSchemaVersion: evidence.SchemaVersion,
		EvidenceJournal:       sealer.JournalEnabled(),
		PolicyVersion:         pol.PolicyVersion,
		PolicyHash:            pol.Hash,
	}

	client.OnConnect(func(c *mqtt.Client) {
		if err := c.Subscribe(cfg.CommandTopic(), 1, func(_ string, payload []byte) {
			mgr.Handle(payload)
		}); err != nil {
			log.Error("subscribe failed", "topic", cfg.CommandTopic(), "error", err)
			return
		}
		log.Info("subscribed to command topic", "topic", cfg.CommandTopic())
		pub.Capabilities(caps)
	})

	if err := client.Connect(); err != nil {
		fatal(fmt.Errorf("mqtt connect: %w", err))
	}
	log.Info("connected to broker", "broker", cfg.BrokerURL())

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	health.StartLoop(ctx, pub, pol, mgr, cfg.RcloneBinary, time.Duration(cfg.HealthIntervalSeconds)*time.Second)

	<-ctx.Done()
	log.Info("shutting down")
	pub.Health(events.HealthEvent{
		Status:           "stopped",
		MaxParallelJobs:  pol.MaxParallelJobs,
		PolicyVersion:    pol.PolicyVersion,
		SupportedActions: mgr.SupportedActions(),
	})
	client.Disconnect()
}

func cmdValidateConfig(args []string) {
	cfgPath := parseConfigFlag("validate-config", args)
	cfg, pol, err := loadAll(cfgPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "invalid:", err)
		os.Exit(1)
	}
	fmt.Printf("ok: worker_id=%s policy_version=%s allowed_actions=%v max_parallel_jobs=%d\n",
		cfg.WorkerID, pol.PolicyVersion, pol.AllowedActions, pol.MaxParallelJobs)
}

func cmdPrintCapabilities(args []string) {
	cfgPath := parseConfigFlag("print-capabilities", args)
	cfg, pol, err := loadAll(cfgPath)
	if err != nil {
		fatal(err)
	}
	caps := events.CapabilitiesEvent{
		EventType:             events.TypeCapabilities,
		WorkerID:              cfg.WorkerID,
		Version:               version.String(),
		SupportedActions:      pol.SupportedActions(),
		MaxParallelJobs:       pol.MaxParallelJobs,
		RcloneAvailable:       rclone.Available(cfg.RcloneBinary),
		EvidenceSchemaVersion: evidence.SchemaVersion,
		EvidenceJournal:       cfg.EvidenceJournal != "",
		PolicyVersion:         pol.PolicyVersion,
		PolicyHash:            pol.Hash,
		Timestamp:             time.Now().UTC().Format(time.RFC3339),
	}
	b, _ := json.MarshalIndent(caps, "", "  ")
	fmt.Println(string(b))
}

func cmdVerify(args []string) {
	fs := flag.NewFlagSet("verify", flag.ExitOnError)
	journal := fs.String("journal", "", "path to the evidence journal (JSONL)")
	_ = fs.Parse(args)
	path := *journal
	if path == "" && fs.NArg() > 0 {
		path = fs.Arg(0)
	}
	if path == "" {
		fmt.Fprintln(os.Stderr, "usage: trailtransfer verify <journal-path>")
		os.Exit(2)
	}
	rep, err := evidence.Verify(path)
	if err != nil {
		fatal(err)
	}
	if rep.OK {
		fmt.Printf("OK: %d record(s), hash chain intact\n", rep.Entries)
		return
	}
	fmt.Printf("FAIL: %d record(s) checked\n", rep.Entries)
	for _, f := range rep.Failures {
		fmt.Println("  - " + f)
	}
	os.Exit(1)
}

func parseConfigFlag(name string, args []string) string {
	fs := flag.NewFlagSet(name, flag.ExitOnError)
	cfgPath := fs.String("config", os.Getenv("TRAILTRANSFER_CONFIG"), "path to a YAML config file (optional)")
	_ = fs.Parse(args)
	return *cfgPath
}

func loadAll(cfgPath string) (*config.Config, *policy.Policy, error) {
	cfg, err := config.Load(cfgPath)
	if err != nil {
		return nil, nil, err
	}
	pol, err := policy.Load(cfg.PolicyFile)
	if err != nil {
		return nil, nil, err
	}
	return cfg, pol, nil
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "error:", err)
	os.Exit(1)
}
