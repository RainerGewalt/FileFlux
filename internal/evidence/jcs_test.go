package evidence

import "testing"

func TestCanonicalJCSSortsAndFormats(t *testing.T) {
	b, err := CanonicalJCS(map[string]any{"b": 1, "a": "x", "c": []int{2, 1}})
	if err != nil {
		t.Fatal(err)
	}
	if got, want := string(b), `{"a":"x","b":1,"c":[2,1]}`; got != want {
		t.Fatalf("got %s want %s", got, want)
	}
}

func TestCanonicalJCSEscaping(t *testing.T) {
	b, _ := CanonicalJCS(map[string]any{"k": "a\"b\\c\nd\te"})
	if got, want := string(b), `{"k":"a\"b\\c\nd\te"}`; got != want {
		t.Fatalf("got %s", got)
	}
}

func TestCanonicalJCSPreservesLargeIntegers(t *testing.T) {
	// > 2^53: must not be rounded via float64.
	b, _ := CanonicalJCS(map[string]any{"bytes": int64(9007199254740993)})
	if got, want := string(b), `{"bytes":9007199254740993}`; got != want {
		t.Fatalf("large int rounded: %s", got)
	}
}
