// Package version exposes the build version, injected at link time via
// -ldflags "-X github.com/RainerGewalt/trailtransfer/internal/version.Version=...".
// It appears in every capabilities and result event.
package version

// Version is the worker version. Overridden at build time; "dev" otherwise.
var Version = "dev"

// String returns the current worker version.
func String() string { return Version }
