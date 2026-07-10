package logging

import "regexp"

// secretPattern matches common credential-carrying assignments so their values
// can be masked before anything is logged or published in an event.
var secretPattern = regexp.MustCompile(`(?i)\b(pass(?:word)?|secret|token|api[_-]?key|key|credential)\s*[=:]\s*\S+`)

// Redact masks credential-looking values in a string. It is deliberately
// conservative: it never widens the string and only replaces the secret token
// after a recognised key.
func Redact(s string) string {
	return secretPattern.ReplaceAllStringFunc(s, func(m string) string {
		// Keep the key and separator, mask the value.
		loc := regexp.MustCompile(`[=:]\s*`).FindStringIndex(m)
		if loc == nil {
			return m
		}
		return m[:loc[1]] + "***"
	})
}
