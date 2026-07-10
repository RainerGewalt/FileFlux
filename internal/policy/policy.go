// Package policy is TrailTransfer's security boundary. Every command is checked
// against a declarative YAML policy before any rclone process is built. A
// rejection is always a clean, published result — never a silent failure.
package policy

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"

	"gopkg.in/yaml.v3"
)

// Policy is the operator-authored allowlist and set of guards for one worker.
type Policy struct {
	WorkerID      string `yaml:"worker_id"`
	PolicyVersion string `yaml:"policy_version"`

	AllowedActions []string `yaml:"allowed_actions"`
	AllowedSources []string `yaml:"allowed_sources"`
	AllowedTargets []string `yaml:"allowed_targets"`

	MaxFileSizeMB   int `yaml:"max_file_size_mb"`
	MaxParallelJobs int `yaml:"max_parallel_jobs"`

	AllowDelete            bool `yaml:"allow_delete"`
	AllowSyncDelete        bool `yaml:"allow_sync_delete"`
	AllowAbsoluteTargets   bool `yaml:"allow_absolute_targets"`
	AllowBroadcastCommands bool `yaml:"allow_broadcast_commands"`
	DryRunDefault          bool `yaml:"dry_run_default"`
	RequireJobID           bool `yaml:"require_job_id"`

	// Hash is the SHA-256 of the exact policy file bytes, filled at load. It
	// appears in result events so an audit can tie a job to the ruleset that
	// governed it, independent of the human-set policy_version.
	Hash string `yaml:"-"`
}

// Load reads and validates a worker policy file.
func Load(path string) (*Policy, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read policy %q: %w", path, err)
	}
	var p Policy
	if err := yaml.Unmarshal(raw, &p); err != nil {
		return nil, fmt.Errorf("parse policy %q: %w", path, err)
	}
	if p.PolicyVersion == "" {
		return nil, fmt.Errorf("policy_version is required")
	}
	if len(p.AllowedActions) == 0 {
		return nil, fmt.Errorf("allowed_actions must list at least one action")
	}
	if p.MaxParallelJobs <= 0 {
		p.MaxParallelJobs = 1
	}
	sum := sha256.Sum256(raw)
	p.Hash = "sha256:" + hex.EncodeToString(sum[:])
	return &p, nil
}

// SupportedActions returns the actions this worker advertises: the allowlisted
// transfer actions plus the always-available control actions.
func (p *Policy) SupportedActions() []string {
	out := append([]string{}, p.AllowedActions...)
	return append(out, "cancel", "status")
}
