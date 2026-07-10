// Package commands models the JSON commands TrailTransfer accepts over MQTT and
// decodes them strictly. MQTT input is untrusted: there is deliberately no
// free-form args/flags field, and unknown fields are rejected.
package commands

// Action is one of the fixed, allowlisted transfer/control actions.
type Action string

const (
	ActionCopy   Action = "copy"
	ActionMove   Action = "move"
	ActionSync   Action = "sync"
	ActionCheck  Action = "check"
	ActionCancel Action = "cancel"
	ActionStatus Action = "status"
)

// Valid reports whether the action is one TrailTransfer understands at all.
func (a Action) Valid() bool {
	switch a {
	case ActionCopy, ActionMove, ActionSync, ActionCheck, ActionCancel, ActionStatus:
		return true
	}
	return false
}

// Executable reports whether the action runs an rclone transfer (as opposed to
// a control action like cancel/status).
func (a Action) Executable() bool {
	switch a {
	case ActionCopy, ActionMove, ActionSync, ActionCheck:
		return true
	}
	return false
}

// Command is the full, closed schema of an incoming command. Recursive and
// DryRun are pointers so "absent" is distinguishable from "false" (the policy
// applies dry_run_default only when the field is absent).
type Command struct {
	JobID       string   `json:"job_id"`
	Action      Action   `json:"action"`
	Source      string   `json:"source,omitempty"`
	Target      string   `json:"target,omitempty"`
	Recursive   *bool    `json:"recursive,omitempty"`
	Filters     []string `json:"filters,omitempty"`
	DryRun      *bool    `json:"dry_run,omitempty"`
	TargetJobID string   `json:"target_job_id,omitempty"` // for cancel/status
}
