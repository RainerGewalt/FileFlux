package evidence

import (
	"bytes"
	"encoding/json"
	"os"
	"sync"
	"time"
)

// Sealer assigns each result record a place in the worker's hash chain and,
// when a journal path is configured, appends it to an fsync'd append-only file
// (the durable, "enduring" evidence record). Chain state is recovered from the
// journal on startup so the chain survives restarts.
//
// With no journal the chain still links records within a process lifetime, but
// resets on restart — enable a journal for full durability.
type Sealer struct {
	workerID   string
	timeSource string

	mu       sync.Mutex
	seq      uint64
	lastHash string
	journal  *os.File
	signer   *Signer
}

// SetSigner enables Ed25519 signing of every sealed record.
func (s *Sealer) SetSigner(signer *Signer) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.signer = signer
}

// Signed reports whether records are being signed.
func (s *Sealer) Signed() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.signer != nil
}

// NewSealer builds a sealer. If journalPath is non-empty the journal is opened
// (created if needed) and chain state is recovered from its last valid record.
func NewSealer(workerID, journalPath string) (*Sealer, error) {
	s := &Sealer{workerID: workerID, timeSource: "system", lastHash: GenesisHash}
	if journalPath == "" {
		return s, nil
	}
	if data, err := os.ReadFile(journalPath); err == nil {
		for _, ln := range bytes.Split(bytes.TrimRight(data, "\n"), []byte("\n")) {
			if len(bytes.TrimSpace(ln)) == 0 {
				continue
			}
			var e Envelope
			if json.Unmarshal(ln, &e) == nil && e.ChainHash != "" {
				s.seq = e.Seq
				s.lastHash = e.ChainHash
			}
		}
	}
	f, err := os.OpenFile(journalPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return nil, err
	}
	s.journal = f
	return s, nil
}

// JournalEnabled reports whether records are persisted durably.
func (s *Sealer) JournalEnabled() bool { return s.journal != nil }

// Seal wraps payload in an envelope, links it into the chain, appends it to the
// journal (if enabled, fsync'd) and returns the marshalled envelope to publish.
func (s *Sealer) Seal(eventType string, payload any, issuer *Issuer) (json.RawMessage, error) {
	pb, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}
	contentHash, err := HashBytes(pb)
	if err != nil {
		return nil, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	env := Envelope{
		SchemaVersion: SchemaVersion,
		WorkerID:      s.workerID,
		Seq:           s.seq + 1,
		PrevHash:      s.lastHash,
		EventType:     eventType,
		RecordedAt:    time.Now().UTC().Format(time.RFC3339Nano),
		TimeSource:    s.timeSource,
		Issuer:        issuer,
		Payload:       json.RawMessage(pb),
		ContentHash:   contentHash,
	}
	ch, err := env.computeChainHash()
	if err != nil {
		return nil, err
	}
	env.ChainHash = ch

	if s.signer != nil {
		sig, err := s.signer.sign(env.ChainHash)
		if err != nil {
			return nil, err
		}
		env.Signature = sig
	}

	out, err := json.Marshal(env)
	if err != nil {
		return nil, err
	}
	if s.journal != nil {
		if _, err := s.journal.Write(append(out, '\n')); err != nil {
			return nil, err
		}
		if err := s.journal.Sync(); err != nil {
			return nil, err
		}
	}
	s.seq = env.Seq
	s.lastHash = env.ChainHash
	return out, nil
}

// Close closes the journal file, if any.
func (s *Sealer) Close() error {
	if s.journal != nil {
		return s.journal.Close()
	}
	return nil
}
