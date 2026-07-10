package evidence

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
)

// VerifyReport is the outcome of checking an evidence journal.
type VerifyReport struct {
	Entries  int      // records checked
	OK       bool     // whole chain intact
	Failures []string // human-readable problems (first break stops the walk)
}

// Verify independently re-checks an evidence journal: for each record it
// recomputes the content hash (payload not altered) and chain hash, and checks
// that prev_hash links to the previous record and that seq increments by one.
// It needs nothing but the file — no broker, no TrailMQ, no keys.
//
// Note: an unsigned chain detects modification, deletion and reordering. An
// actor who can rewrite the *entire* suffix of the journal can still forge a
// consistent chain; to defend against that, enable signatures (v0.3) and/or
// anchor the latest chain_hash in external immutable storage (WORM/TSA/TrailMQ).
func Verify(journalPath string) (VerifyReport, error) {
	data, err := os.ReadFile(journalPath)
	if err != nil {
		return VerifyReport{}, err
	}
	rep := VerifyReport{OK: true}
	prev := GenesisHash
	var expectedSeq uint64 = 1

	for _, ln := range bytes.Split(bytes.TrimRight(data, "\n"), []byte("\n")) {
		if len(bytes.TrimSpace(ln)) == 0 {
			continue
		}
		idx := rep.Entries // 0-based index of this record
		rep.Entries++

		var e Envelope
		if err := json.Unmarshal(ln, &e); err != nil {
			rep.fail(idx, "invalid JSON: "+err.Error())
			break
		}
		if got, _ := HashBytes(e.Payload); got != e.ContentHash {
			rep.fail(idx, "content_hash mismatch — payload was altered")
			break
		}
		if want, _ := e.computeChainHash(); want != e.ChainHash {
			rep.fail(idx, "chain_hash mismatch — envelope core was altered")
			break
		}
		if e.PrevHash != prev {
			rep.fail(idx, fmt.Sprintf("prev_hash break — expected %s, got %s (record removed or reordered)", prev, e.PrevHash))
			break
		}
		if e.Seq != expectedSeq {
			rep.fail(idx, fmt.Sprintf("seq gap — expected %d, got %d", expectedSeq, e.Seq))
			break
		}
		prev = e.ChainHash
		expectedSeq++
	}
	return rep, nil
}

func (r *VerifyReport) fail(index int, msg string) {
	r.OK = false
	r.Failures = append(r.Failures, fmt.Sprintf("record #%d: %s", index, msg))
}
