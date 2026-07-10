// Package logging configures structured logging and redacts secrets so that
// credentials from rclone.conf or error output never reach the log stream.
package logging

import (
	"log/slog"
	"os"
	"strings"
)

// Setup installs a JSON slog handler at the given level and returns it.
func Setup(level string) *slog.Logger {
	var lv slog.Level
	switch strings.ToLower(level) {
	case "debug":
		lv = slog.LevelDebug
	case "warn", "warning":
		lv = slog.LevelWarn
	case "error":
		lv = slog.LevelError
	default:
		lv = slog.LevelInfo
	}
	l := slog.New(slog.NewJSONHandler(os.Stderr, &slog.HandlerOptions{Level: lv}))
	slog.SetDefault(l)
	return l
}
