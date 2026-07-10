package evidence

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func sealN(t *testing.T, path string, n int) {
	t.Helper()
	s, err := NewSealer("w1", path)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < n; i++ {
		if _, err := s.Seal("result", map[string]any{"status": "completed", "n": i}, &Issuer{Auth: "none"}); err != nil {
			t.Fatal(err)
		}
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestSealAndVerifyIntact(t *testing.T) {
	path := filepath.Join(t.TempDir(), "journal.jsonl")
	sealN(t, path, 3)
	rep, err := Verify(path, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !rep.OK || rep.Entries != 3 {
		t.Fatalf("expected 3 intact records, got %+v", rep)
	}
}

func TestVerifyDetectsPayloadTamper(t *testing.T) {
	path := filepath.Join(t.TempDir(), "journal.jsonl")
	s, _ := NewSealer("w1", path)
	_, _ = s.Seal("result", map[string]any{"status": "completed", "amount": 1}, nil)
	_, _ = s.Seal("result", map[string]any{"status": "completed", "amount": 2}, nil)
	s.Close()

	data, _ := os.ReadFile(path)
	os.WriteFile(path, bytes.Replace(data, []byte(`"amount":1`), []byte(`"amount":9`), 1), 0o600)

	rep, _ := Verify(path, nil)
	if rep.OK {
		t.Fatal("verify must detect payload tampering")
	}
}

func TestVerifyDetectsDeletion(t *testing.T) {
	path := filepath.Join(t.TempDir(), "journal.jsonl")
	sealN(t, path, 3)

	data, _ := os.ReadFile(path)
	lines := bytes.Split(bytes.TrimRight(data, "\n"), []byte("\n"))
	kept := append([][]byte{lines[0]}, lines[2]) // drop the middle record
	os.WriteFile(path, bytes.Join(kept, []byte("\n")), 0o600)

	rep, _ := Verify(path, nil)
	if rep.OK {
		t.Fatal("verify must detect a removed record")
	}
}

func TestSealerRecoversChainAcrossRestart(t *testing.T) {
	path := filepath.Join(t.TempDir(), "journal.jsonl")
	sealN(t, path, 1)

	s2, _ := NewSealer("w1", path) // reopen: recover chain state
	raw, _ := s2.Seal("result", map[string]any{"status": "completed"}, nil)
	s2.Close()

	var e Envelope
	if err := json.Unmarshal(raw, &e); err != nil {
		t.Fatal(err)
	}
	if e.Seq != 2 {
		t.Fatalf("expected seq 2 after recovery, got %d", e.Seq)
	}
	if rep, _ := Verify(path, nil); !rep.OK || rep.Entries != 2 {
		t.Fatalf("chain broken after recovery: %+v", rep)
	}
}
