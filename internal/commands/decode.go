package commands

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
)

// Decode parses a command payload with strict schema enforcement: unknown
// fields are rejected, exactly one JSON value is required, job_id must be
// present, and the action must be known.
func Decode(payload []byte) (*Command, error) {
	dec := json.NewDecoder(bytes.NewReader(payload))
	dec.DisallowUnknownFields()

	var c Command
	if err := dec.Decode(&c); err != nil {
		return nil, fmt.Errorf("schema: %w", err)
	}
	if dec.More() {
		return nil, errors.New("schema: unexpected trailing data after command")
	}
	if c.JobID == "" {
		return nil, errors.New("schema: job_id is required")
	}
	if !c.Action.Valid() {
		return nil, fmt.Errorf("schema: unknown action %q", c.Action)
	}
	if (c.Action == ActionCancel || c.Action == ActionStatus) && c.TargetJobID == "" {
		return nil, fmt.Errorf("schema: %s requires target_job_id", c.Action)
	}
	return &c, nil
}

// PeekJobID best-effort extracts job_id from an otherwise invalid payload so a
// rejection can still be routed to the right job topic.
func PeekJobID(payload []byte) string {
	var m struct {
		JobID string `json:"job_id"`
	}
	_ = json.Unmarshal(payload, &m)
	return m.JobID
}
