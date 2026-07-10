// Package rclone builds a hard-coded rclone argument vector (never a shell
// string) from a validated command and runs it as a subprocess, turning its
// JSON stats stream into progress and a final outcome.
package rclone

import (
	"fmt"
	"strings"

	"github.com/RainerGewalt/trailtransfer/internal/commands"
)

// Runner holds the fixed execution parameters for this worker.
type Runner struct {
	RclonePath   string
	RcloneConfig string
	MaxSizeMB    int
}

// subcommand maps an executable action to its rclone subcommand. The mapping is
// closed: anything not listed is refused before a process is ever built.
func subcommand(a commands.Action) (string, error) {
	switch a {
	case commands.ActionCopy:
		return "copy", nil
	case commands.ActionMove:
		return "move", nil
	case commands.ActionSync:
		return "sync", nil
	case commands.ActionCheck:
		return "check", nil
	default:
		return "", fmt.Errorf("action %q is not executable", a)
	}
}

// BuildArgs assembles the rclone argument vector. Source/target are expected to
// have already passed policy validation. Filters become --include patterns
// (validated), never raw pass-through, and there is no way to inject arbitrary
// flags.
func BuildArgs(r *Runner, c *commands.Command, dryRun bool) ([]string, error) {
	sub, err := subcommand(c.Action)
	if err != nil {
		return nil, err
	}
	args := []string{
		sub, c.Source, c.Target,
		"--config", r.RcloneConfig,
		"--use-json-log",
		"--stats", "500ms",
		"--stats-log-level", "NOTICE",
	}
	if r.MaxSizeMB > 0 {
		args = append(args, "--max-size", fmt.Sprintf("%dM", r.MaxSizeMB))
	}
	if c.Recursive != nil && !*c.Recursive {
		args = append(args, "--max-depth", "1")
	}
	if dryRun {
		args = append(args, "--dry-run")
	}
	for _, f := range c.Filters {
		if err := validateFilterPattern(f); err != nil {
			return nil, err
		}
		args = append(args, "--include", f)
	}
	return args, nil
}

// validateFilterPattern rejects filter values that could be misread as flags or
// carry control characters. rclone parses these as glob patterns; we still keep
// them well-formed and flag-safe.
func validateFilterPattern(f string) error {
	if f == "" {
		return fmt.Errorf("filter pattern must not be empty")
	}
	if len(f) > 256 {
		return fmt.Errorf("filter pattern too long")
	}
	if strings.HasPrefix(f, "-") {
		return fmt.Errorf("filter pattern %q must not start with '-'", f)
	}
	if strings.ContainsAny(f, "\x00\n\r") {
		return fmt.Errorf("filter pattern contains control characters")
	}
	return nil
}
