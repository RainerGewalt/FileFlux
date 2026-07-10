package evidence

import "encoding/json"

// SchemaVersion is the evidence envelope schema version. Bump on any breaking
// change to the envelope shape; it travels in every sealed record.
const SchemaVersion = "1"

// GenesisHash is the prev_hash of the first record in a chain.
const GenesisHash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"

// Issuer records who caused a record to exist. In v0.2 this is self-declared by
// the command (the `operator` field), because MQTT 3.1.1 does not forward the
// publisher's authenticated identity to subscribers. Strong attribution is a
// broker/governance responsibility (ACL, mTLS, MQTT 5 user-properties, TrailMQ).
type Issuer struct {
	Auth     string `json:"auth"`               // "self-declared" | "none"
	Identity string `json:"identity,omitempty"` // e.g. operator name
}

// Signature is an optional detached signature over the envelope core (v0.3+).
type Signature struct {
	Alg   string `json:"alg"` // e.g. "ed25519"
	KeyID string `json:"key_id"`
	Sig   string `json:"sig"`
}

// Envelope wraps a domain event with the fields that make it audit-ready:
// a monotonic sequence number and a link to the previous record (tamper
// evidence), a content hash (JCS of the payload), and a chain hash over the
// envelope core. Together they let Verify prove that no record was altered,
// removed or reordered.
type Envelope struct {
	SchemaVersion string          `json:"schema_version"`
	WorkerID      string          `json:"worker_id"`
	Seq           uint64          `json:"seq"`
	PrevHash      string          `json:"prev_hash"`
	EventType     string          `json:"event_type"`
	RecordedAt    string          `json:"recorded_at"` // ISO 8601 UTC
	TimeSource    string          `json:"time_source"` // "system" (see docs on NTP/TSA)
	Issuer        *Issuer         `json:"issuer,omitempty"`
	Payload       json.RawMessage `json:"payload"`
	ContentHash   string          `json:"content_hash"`
	ChainHash     string          `json:"chain_hash"`
	Signature     *Signature      `json:"signature,omitempty"`
}

// chainCore is the exact set of fields the chain hash covers. Kept as an
// ordered-by-JCS map so seal and verify compute identical bytes.
func (e *Envelope) chainCore() map[string]any {
	core := map[string]any{
		"schema_version": e.SchemaVersion,
		"worker_id":      e.WorkerID,
		"seq":            e.Seq,
		"prev_hash":      e.PrevHash,
		"event_type":     e.EventType,
		"recorded_at":    e.RecordedAt,
		"time_source":    e.TimeSource,
		"content_hash":   e.ContentHash,
	}
	if e.Issuer != nil {
		core["issuer"] = e.Issuer
	}
	return core
}

// computeChainHash returns the SHA-256 over the JCS of the envelope core.
func (e *Envelope) computeChainHash() (string, error) {
	return Hash(e.chainCore())
}
