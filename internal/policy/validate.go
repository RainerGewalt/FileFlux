package policy

import (
	"fmt"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/RainerGewalt/trailtransfer/internal/commands"
)

// Decision is the outcome of evaluating a command against the policy.
type Decision struct {
	Allowed bool
	Reason  string // machine-readable, e.g. "policy_violation"
	Detail  string // human-readable explanation
}

var remotePrefix = regexp.MustCompile(`^[A-Za-z0-9_.-]+:`)

// Evaluate checks an executable command (copy/move/sync/check) against the
// policy. Control actions (cancel/status) are handled before policy and must
// not be passed here.
func (p *Policy) Evaluate(c *commands.Command) Decision {
	if p.RequireJobID && c.JobID == "" {
		return deny("job_id is required by policy")
	}
	if !p.actionAllowed(c.Action) {
		return deny("action %q is not in allowed_actions", c.Action)
	}
	if c.Action == commands.ActionSync && !p.AllowSyncDelete {
		return deny("sync (which may delete on the target) is not permitted: allow_sync_delete is false")
	}

	if strings.Contains(c.Source, "..") {
		return deny("source %q must not contain '..'", c.Source)
	}
	if !p.sourceAllowed(c.Source) {
		return deny("source %q is not under any allowed_sources root", c.Source)
	}

	if isLocalAbsolute(c.Target) && !p.AllowAbsoluteTargets {
		return deny("absolute local target %q is not permitted: allow_absolute_targets is false", c.Target)
	}
	if !p.targetAllowed(c.Target) {
		return deny("target %q is not in allowed_targets", c.Target)
	}
	return Decision{Allowed: true}
}

func (p *Policy) actionAllowed(a commands.Action) bool {
	for _, x := range p.AllowedActions {
		if x == string(a) {
			return true
		}
	}
	return false
}

// sourceAllowed normalises the source and requires it to sit at or beneath one
// of the allowed roots, which defeats lexical path traversal.
func (p *Policy) sourceAllowed(src string) bool {
	if src == "" {
		return false
	}
	clean := filepath.Clean(src)
	for _, root := range p.AllowedSources {
		r := filepath.Clean(root)
		if clean == r || strings.HasPrefix(clean, r+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

// targetAllowed permits a target that equals an allowed target or sits beneath
// it (allowed "sftp:/upload" also permits "sftp:/upload/sub").
func (p *Policy) targetAllowed(target string) bool {
	if target == "" {
		return false
	}
	for _, t := range p.AllowedTargets {
		base := strings.TrimSuffix(t, "/")
		if target == t || target == base || strings.HasPrefix(target, base+"/") {
			return true
		}
	}
	return false
}

// isLocalAbsolute reports whether target is an absolute local path (as opposed
// to an rclone "remote:path" reference).
func isLocalAbsolute(target string) bool {
	if remotePrefix.MatchString(target) {
		return false
	}
	return filepath.IsAbs(target)
}

func deny(format string, args ...any) Decision {
	return Decision{Allowed: false, Reason: "policy_violation", Detail: fmt.Sprintf(format, args...)}
}
