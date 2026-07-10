package evidence

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func writeKeyPair(t *testing.T, dir string) (keyPath, pubPath string) {
	t.Helper()
	priv, pub, _, err := GenerateKeyPair()
	if err != nil {
		t.Fatal(err)
	}
	keyPath = filepath.Join(dir, "k.key")
	pubPath = filepath.Join(dir, "k.pub")
	if err := os.WriteFile(keyPath, priv, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(pubPath, pub, 0o644); err != nil {
		t.Fatal(err)
	}
	return keyPath, pubPath
}

func TestSignedChainVerifies(t *testing.T) {
	dir := t.TempDir()
	keyPath, pubPath := writeKeyPair(t, dir)
	signer, err := LoadSigner(keyPath)
	if err != nil {
		t.Fatal(err)
	}
	pub, err := LoadPublicKey(pubPath)
	if err != nil {
		t.Fatal(err)
	}

	path := filepath.Join(dir, "journal.jsonl")
	s, _ := NewSealer("w1", path)
	s.SetSigner(signer)
	for i := 0; i < 3; i++ {
		if _, err := s.Seal("result", map[string]any{"status": "completed", "n": i}, nil); err != nil {
			t.Fatal(err)
		}
	}
	s.Close()

	rep, err := Verify(path, pub)
	if err != nil {
		t.Fatal(err)
	}
	if !rep.OK || !rep.Signed || rep.SignaturesChecked != 3 {
		t.Fatalf("expected 3 verified signatures on an intact chain, got %+v", rep)
	}
}

func TestSignatureDetectsForgedRecord(t *testing.T) {
	dir := t.TempDir()
	keyPath, pubPath := writeKeyPair(t, dir)
	signer, _ := LoadSigner(keyPath)
	pub, _ := LoadPublicKey(pubPath)

	path := filepath.Join(dir, "journal.jsonl")
	s, _ := NewSealer("w1", path)
	s.SetSigner(signer)
	_, _ = s.Seal("result", map[string]any{"status": "completed", "amount": 1}, nil)
	s.Close()

	// Forge: change the payload AND recompute content_hash + chain_hash so the
	// unsigned checks would pass — only the signature (no private key) exposes it.
	data, _ := os.ReadFile(path)
	var e Envelope
	if err := json.Unmarshal(data, &e); err != nil {
		t.Fatal(err)
	}
	e.Payload = []byte(`{"status":"completed","amount":999999}`)
	e.ContentHash, _ = HashBytes(e.Payload)
	e.ChainHash, _ = e.computeChainHash()
	// signature left as-is (now over the old chain hash) → must fail
	forged, _ := json.Marshal(&e)
	os.WriteFile(path, append(forged, '\n'), 0o600)

	// Without the key: unsigned checks pass (attacker rewrote the chain).
	if rep, _ := Verify(path, nil); !rep.OK {
		t.Fatal("unsigned verify unexpectedly failed on a self-consistent forgery")
	}
	// With the key: the forged record's signature does not verify.
	if rep, _ := Verify(path, pub); rep.OK {
		t.Fatal("signature verification must reject a forged record")
	}
}
