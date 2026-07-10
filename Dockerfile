# syntax=docker/dockerfile:1

# ---- build ----
FROM golang:1.26 AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
ARG VERSION=dev
RUN CGO_ENABLED=0 go build -trimpath \
    -ldflags "-s -w -X github.com/RainerGewalt/trailtransfer/internal/version.Version=${VERSION}" \
    -o /out/trailtransfer ./cmd/trailtransfer

# ---- rclone (pinned) — the transfer engine, called as a subprocess ----
FROM rclone/rclone:1.68 AS rclone

# ---- runtime ----
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build  /out/trailtransfer      /usr/local/bin/trailtransfer
COPY --from=rclone /usr/local/bin/rclone   /usr/local/bin/rclone
USER nonroot:nonroot
# /config: worker-policy.yaml + rclone.conf (read-only). /data: source files.
VOLUME ["/config", "/data"]
ENTRYPOINT ["/usr/local/bin/trailtransfer"]
CMD ["run"]
