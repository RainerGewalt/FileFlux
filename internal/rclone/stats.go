package rclone

import "encoding/json"

// logLine is the subset of an rclone --use-json-log line we care about.
type logLine struct {
	Level string `json:"level"`
	Msg   string `json:"msg"`
	Stats *struct {
		Bytes          int64 `json:"bytes"`
		TotalBytes     int64 `json:"totalBytes"`
		Transfers      int   `json:"transfers"`
		TotalTransfers int   `json:"totalTransfers"`
		Errors         int   `json:"errors"`
	} `json:"stats"`
}

// parseLine decodes one JSON log line; ok is false for non-JSON output.
func parseLine(b []byte) (logLine, bool) {
	var l logLine
	if err := json.Unmarshal(b, &l); err != nil {
		return logLine{}, false
	}
	return l, true
}
