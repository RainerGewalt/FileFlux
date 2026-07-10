package jobs

import (
	"context"
	"encoding/json"
	"io"
	"log/slog"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/RainerGewalt/trailtransfer/internal/commands"
	"github.com/RainerGewalt/trailtransfer/internal/config"
	"github.com/RainerGewalt/trailtransfer/internal/events"
	"github.com/RainerGewalt/trailtransfer/internal/policy"
	"github.com/RainerGewalt/trailtransfer/internal/rclone"
)

// recorder is a fake MQTT sink that captures every published message.
type recorder struct {
	mu   sync.Mutex
	msgs []struct {
		topic   string
		payload []byte
	}
}

func (r *recorder) Publish(topic string, _ byte, _ bool, payload []byte) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.msgs = append(r.msgs, struct {
		topic   string
		payload []byte
	}{topic, append([]byte(nil), payload...)})
	return nil
}

// waitResult polls for the result event on the given job and returns it parsed.
func (r *recorder) waitResult(t *testing.T, jobID string) map[string]any {
	t.Helper()
	suffix := "jobs/" + jobID + "/result"
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		r.mu.Lock()
		for _, m := range r.msgs {
			if strings.HasSuffix(m.topic, suffix) {
				var out map[string]any
				_ = json.Unmarshal(m.payload, &out)
				r.mu.Unlock()
				return out
			}
		}
		r.mu.Unlock()
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("no result event for %s", jobID)
	return nil
}

type fakeRunner struct {
	out rclone.Outcome
	err error
}

func (f fakeRunner) Run(_ context.Context, _ *commands.Command, _ bool, onProgress func(rclone.Progress)) (rclone.Outcome, error) {
	if onProgress != nil {
		onProgress(rclone.Progress{FilesTotal: 1, FilesTransferred: 1, BytesTotal: 100, BytesTransferred: 100})
	}
	return f.out, f.err
}

// recordingRunner captures the dryRun flag it was invoked with.
type recordingRunner struct {
	called bool
	dryRun bool
}

func (r *recordingRunner) Run(_ context.Context, _ *commands.Command, dryRun bool, _ func(rclone.Progress)) (rclone.Outcome, error) {
	r.called = true
	r.dryRun = dryRun
	return rclone.Outcome{}, nil
}

func newTestManager(runner Runner) (*Manager, *recorder) {
	cfg := &config.Config{WorkerID: "worker-01", TopicPrefix: "trailtransfer"}
	pol := &policy.Policy{
		PolicyVersion:   "1",
		AllowedActions:  []string{"copy"},
		AllowedSources:  []string{"/data/input"},
		AllowedTargets:  []string{"minio-demo:trailtransfer"},
		MaxParallelJobs: 2,
		RequireJobID:    true,
		DryRunDefault:   true,
	}
	rec := &recorder{}
	pub := events.NewPublisher(rec, cfg, "test")
	log := slog.New(slog.NewTextHandler(io.Discard, nil))
	return NewManager(pol, pub, runner, log), rec
}

func TestHandleCompletesValidCopy(t *testing.T) {
	m, rec := newTestManager(fakeRunner{out: rclone.Outcome{FilesTotal: 1, FilesTransferred: 1, BytesTransferred: 100}})
	m.Handle([]byte(`{"job_id":"job-001","action":"copy","source":"/data/input","target":"minio-demo:trailtransfer"}`))

	res := rec.waitResult(t, "job-001")
	if res["status"] != "completed" {
		t.Fatalf("expected completed, got %v", res["status"])
	}
	if !strings.HasPrefix(res["command_hash"].(string), "sha256:") {
		t.Fatalf("missing command_hash: %v", res["command_hash"])
	}
	if !strings.HasPrefix(res["result_hash"].(string), "sha256:") {
		t.Fatalf("missing result_hash: %v", res["result_hash"])
	}
	if res["policy_version"] != "1" || res["worker_version"] != "test" {
		t.Fatalf("evidence fields wrong: %+v", res)
	}
}

func TestHandleRejectsDisallowedSource(t *testing.T) {
	m, rec := newTestManager(fakeRunner{})
	m.Handle([]byte(`{"job_id":"job-002","action":"copy","source":"/etc","target":"minio-demo:trailtransfer"}`))
	res := rec.waitResult(t, "job-002")
	if res["status"] != "rejected" || res["reason"] != "policy_violation" {
		t.Fatalf("expected policy rejection, got %+v", res)
	}
	if _, ran := res["rclone_exit_code"]; ran {
		t.Fatal("rejected job must not have run rclone")
	}
}

func TestHandleRejectsFailedExit(t *testing.T) {
	m, rec := newTestManager(fakeRunner{out: rclone.Outcome{ExitCode: 1, Errors: []string{"boom"}}})
	m.Handle([]byte(`{"job_id":"job-003","action":"copy","source":"/data/input","target":"minio-demo:trailtransfer"}`))
	res := rec.waitResult(t, "job-003")
	if res["status"] != "failed" {
		t.Fatalf("nonzero rclone exit must fail the job, got %v", res["status"])
	}
}

func TestDryRunDefaultApplied(t *testing.T) {
	rr := &recordingRunner{}
	m, rec := newTestManager(rr) // test policy has DryRunDefault: true
	m.Handle([]byte(`{"job_id":"job-dr","action":"copy","source":"/data/input","target":"minio-demo:trailtransfer"}`))
	rec.waitResult(t, "job-dr")
	if !rr.called {
		t.Fatal("runner was not invoked")
	}
	if !rr.dryRun {
		t.Fatal("dry_run_default:true must make a command with absent dry_run run as a dry-run")
	}
}

func TestHandleDedupesJobID(t *testing.T) {
	m, rec := newTestManager(fakeRunner{out: rclone.Outcome{FilesTransferred: 1}})
	payload := []byte(`{"job_id":"job-001","action":"copy","source":"/data/input","target":"minio-demo:trailtransfer"}`)
	m.Handle(payload)
	rec.waitResult(t, "job-001")
	// same job_id again -> duplicate rejection (last result on that topic)
	m.Handle(payload)
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		rec.mu.Lock()
		var last map[string]any
		for _, msg := range rec.msgs {
			if strings.HasSuffix(msg.topic, "jobs/job-001/result") {
				_ = json.Unmarshal(msg.payload, &last)
			}
		}
		rec.mu.Unlock()
		if last != nil && last["reason"] == "duplicate_job_id" {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatal("expected duplicate_job_id rejection on repeat")
}
