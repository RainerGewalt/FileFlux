// Package evidence produces canonical, hashable representations of commands and
// results. Canonicalisation is mandatory: without deterministic serialisation
// the hashes that back the audit trail would be worthless.
package evidence

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
)

// Canonical returns a deterministic JSON encoding of v. It round-trips through
// a generic value so that encoding/json emits map keys in sorted order,
// producing a stable byte sequence regardless of struct field order.
func Canonical(v any) ([]byte, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	var generic any
	if err := json.Unmarshal(raw, &generic); err != nil {
		return nil, err
	}
	return json.Marshal(generic)
}

// Hash returns the SHA-256 of the canonical encoding of v, prefixed "sha256:".
func Hash(v any) (string, error) {
	b, err := Canonical(v)
	if err != nil {
		return "", err
	}
	return sum(b), nil
}

// HashBytes canonicalises a raw JSON payload (falling back to the raw bytes if
// it is not valid JSON) and returns its SHA-256. Used to hash a command that
// failed schema validation so even a rejection is provable.
func HashBytes(payload []byte) (string, error) {
	var generic any
	if err := json.Unmarshal(payload, &generic); err != nil {
		return sum(payload), nil
	}
	b, err := json.Marshal(generic)
	if err != nil {
		return sum(payload), nil
	}
	return sum(b), nil
}

func sum(b []byte) string {
	h := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(h[:])
}
