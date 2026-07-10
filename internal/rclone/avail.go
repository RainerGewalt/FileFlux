package rclone

import "os/exec"

// Available reports whether the rclone binary can be resolved on PATH (or as an
// absolute path). Surfaced in health/capabilities so a misconfigured worker is
// visible rather than only failing at the first job.
func Available(binary string) bool {
	if binary == "" {
		binary = "rclone"
	}
	_, err := exec.LookPath(binary)
	return err == nil
}
