// Package evidence produces canonical, hashable, chain-linked records of
// TrailTransfer jobs. Canonicalisation follows RFC 8785 (JCS); result records
// are sealed into a tamper-evident hash chain (see Sealer) and can be
// independently checked (see Verify) without any external service.
package evidence

import (
	"crypto/sha256"
	"encoding/hex"
)

// Hash returns the SHA-256 of the RFC 8785 canonical encoding of v.
func Hash(v any) (string, error) {
	b, err := CanonicalJCS(v)
	if err != nil {
		return "", err
	}
	return sum(b), nil
}

// HashBytes canonicalises a raw JSON payload (falling back to the raw bytes if
// it is not valid JSON) and returns its SHA-256.
func HashBytes(payload []byte) (string, error) {
	b, err := canonicalizeJSON(payload)
	if err != nil {
		return sum(payload), nil
	}
	return sum(b), nil
}

func sum(b []byte) string {
	h := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(h[:])
}
