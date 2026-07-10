package commands

import "testing"

func TestDecodeValidCopy(t *testing.T) {
	c, err := Decode([]byte(`{"job_id":"job-001","action":"copy","source":"/data/input","target":"sftp-demo:/upload","recursive":true,"filters":["*.jpg"],"dry_run":false}`))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if c.JobID != "job-001" || c.Action != ActionCopy {
		t.Fatalf("unexpected command: %+v", c)
	}
	if c.DryRun == nil || *c.DryRun != false {
		t.Fatalf("dry_run should be present and false, got %v", c.DryRun)
	}
	if c.Recursive == nil || *c.Recursive != true {
		t.Fatalf("recursive should be present and true")
	}
}

func TestDecodeRejectsUnknownField(t *testing.T) {
	_, err := Decode([]byte(`{"job_id":"j","action":"copy","source":"/a","target":"r:/b","extra_args":"--delete"}`))
	if err == nil {
		t.Fatal("expected rejection of unknown field (no free-form args allowed)")
	}
}

func TestDecodeRequiresJobID(t *testing.T) {
	if _, err := Decode([]byte(`{"action":"copy","source":"/a","target":"r:/b"}`)); err == nil {
		t.Fatal("expected error for missing job_id")
	}
}

func TestDecodeRejectsUnknownAction(t *testing.T) {
	if _, err := Decode([]byte(`{"job_id":"j","action":"exec","source":"/a","target":"r:/b"}`)); err == nil {
		t.Fatal("expected error for unknown action")
	}
}

func TestDecodeCancelRequiresTarget(t *testing.T) {
	if _, err := Decode([]byte(`{"job_id":"c1","action":"cancel"}`)); err == nil {
		t.Fatal("expected cancel to require target_job_id")
	}
	if _, err := Decode([]byte(`{"job_id":"c1","action":"cancel","target_job_id":"job-1"}`)); err != nil {
		t.Fatalf("valid cancel should decode: %v", err)
	}
}

func TestDryRunAbsentIsNil(t *testing.T) {
	c, err := Decode([]byte(`{"job_id":"j","action":"copy","source":"/a","target":"r:/b"}`))
	if err != nil {
		t.Fatal(err)
	}
	if c.DryRun != nil {
		t.Fatal("absent dry_run must stay nil so policy default can apply")
	}
}
