# Archived: FileFlux (Rust)

This directory holds the original **FileFlux** Rust implementation, kept as a
**migration reference only**. It is **not** built, tested, or shipped — there is
no Rust step in CI or in the Dockerfile.

The active product is **TrailTransfer**, implemented in Go under `cmd/` and
`internal/`. See [`../../docs/MIGRATION.md`](../../docs/MIGRATION.md) for what
changed and why.

Do not add new Rust code here. If this reference is no longer useful, the whole
`legacy/` directory can be deleted (its history remains in git).
