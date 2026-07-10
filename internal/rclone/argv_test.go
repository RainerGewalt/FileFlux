package rclone

import (
	"strings"
	"testing"

	"github.com/RainerGewalt/trailtransfer/internal/commands"
)

func runner() *Runner {
	return &Runner{RclonePath: "rclone", RcloneConfig: "/config/rclone.conf", MaxSizeMB: 500}
}

func ptr[T any](v T) *T { return &v }

func TestBuildArgsCopy(t *testing.T) {
	c := &commands.Command{JobID: "j", Action: commands.ActionCopy, Source: "/data/input", Target: "sftp-demo:/upload"}
	args, err := BuildArgs(runner(), c, false)
	if err != nil {
		t.Fatal(err)
	}
	if args[0] != "copy" || args[1] != "/data/input" || args[2] != "sftp-demo:/upload" {
		t.Fatalf("unexpected leading args: %v", args)
	}
	joined := strings.Join(args, " ")
	for _, want := range []string{"--config /config/rclone.conf", "--use-json-log", "--stats 500ms", "--max-size 500M"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("missing %q in %q", want, joined)
		}
	}
	if strings.Contains(joined, "--dry-run") {
		t.Fatal("dry-run should not be present")
	}
}

func TestBuildArgsDryRunAndNonRecursive(t *testing.T) {
	c := &commands.Command{JobID: "j", Action: commands.ActionCopy, Source: "/data/input", Target: "r:/b", Recursive: ptr(false)}
	args, _ := BuildArgs(runner(), c, true)
	joined := strings.Join(args, " ")
	if !strings.Contains(joined, "--dry-run") {
		t.Fatal("expected --dry-run")
	}
	if !strings.Contains(joined, "--max-depth 1") {
		t.Fatal("expected --max-depth 1 for non-recursive")
	}
}

func TestBuildArgsRejectsFlagLikeFilter(t *testing.T) {
	c := &commands.Command{JobID: "j", Action: commands.ActionCopy, Source: "/a", Target: "r:/b", Filters: []string{"--delete-during"}}
	if _, err := BuildArgs(runner(), c, false); err == nil {
		t.Fatal("filter starting with '-' must be rejected (no flag injection)")
	}
}

func TestBuildArgsRejectsControlCharFilter(t *testing.T) {
	c := &commands.Command{JobID: "j", Action: commands.ActionCopy, Source: "/a", Target: "r:/b", Filters: []string{"a\nb"}}
	if _, err := BuildArgs(runner(), c, false); err == nil {
		t.Fatal("filter with control chars must be rejected")
	}
}

func TestBuildArgsRejectsNonExecutableAction(t *testing.T) {
	c := &commands.Command{JobID: "j", Action: commands.ActionCancel, TargetJobID: "x"}
	if _, err := BuildArgs(runner(), c, false); err == nil {
		t.Fatal("cancel is not executable and must not build args")
	}
}
