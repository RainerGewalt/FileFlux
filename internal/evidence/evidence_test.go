package evidence

import "testing"

func TestHashBytesKeyOrderIndependent(t *testing.T) {
	a, _ := HashBytes([]byte(`{"a":1,"b":2,"c":[1,2,3]}`))
	b, _ := HashBytes([]byte(`{"c":[1,2,3],"b":2,"a":1}`))
	if a != b {
		t.Fatalf("canonical hash must be key-order independent: %s vs %s", a, b)
	}
	if a[:7] != "sha256:" {
		t.Fatalf("hash must be sha256-prefixed: %s", a)
	}
}

func TestHashArrayOrderMatters(t *testing.T) {
	a, _ := HashBytes([]byte(`{"x":[1,2]}`))
	b, _ := HashBytes([]byte(`{"x":[2,1]}`))
	if a == b {
		t.Fatal("array order is semantically significant and must change the hash")
	}
}

func TestHashStruct(t *testing.T) {
	type R struct {
		Status     string `json:"status"`
		ResultHash string `json:"result_hash,omitempty"`
	}
	// result_hash is empty (omitempty), so it is excluded and the hash is stable.
	h1, err := Hash(R{Status: "completed"})
	if err != nil {
		t.Fatal(err)
	}
	h2, _ := Hash(R{Status: "completed"})
	if h1 != h2 {
		t.Fatal("hash of identical structs must match")
	}
}
