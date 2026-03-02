# Lifetimefs Filesystem

This README describes how the filesystem module works today.

## High-level flow

1. `lifetimefs mount <mountpoint>` is invoked from CLI.
2. The CLI client sends a `mount` command over Unix socket (`/tmp/lifetimefs.sock`).
3. The service receives the command and creates `Lifetimefs`.
4. `Lifetimefs::new()` resolves storage for the mountpoint.
5. `Lifetimefs::mount()` starts a FUSE background session with `fuser::spawn_mount2`.

When the filesystem is unmounted, `destroy()` is called and the service removes the corresponding session from memory.

## Storage model

All filesystem state is stored under:

- `~/.lifetimefs`

Instances are under:

- `~/.lifetimefs/instances/<instance_id>`

`<instance_id>` is a SHA-256 hash of the canonical mountpoint path.

Each instance has this layout:

- `metadata.db` (currently created as a file placeholder)
- `data/`
- `head/`
- `config.json`

Planned meaning of these paths:

- `metadata.db`: SQLite metadata database (inode map, path map, version pointers).
- `data/`: immutable history objects (diff/snapshot history store).
- `head/`: persistent materialized latest file contents (fast read tier).

`config.json` stores:

- `version`
- `id` (the hashed instance id)
- `mountpoint` (original path)
- `canonical_mountpoint` (canonicalized path)

## How instances are created/reused

`Storage::get_or_create_instance_for_mountpoint()` does the following:

1. Canonicalizes the requested mountpoint.
2. Computes its instance id (SHA-256).
3. Ensures instance directories/files exist.
4. Writes/updates `config.json` metadata.

This gives stable per-mountpoint storage across runs.

## Mount listing

`Storage::list_instances()` scans `~/.lifetimefs/instances` and returns:

- `canonical_mountpoint` from `config.json` when available
- otherwise, the directory name (fallback for old instances)

The CLI command `list-mounts` prints these values in sorted order.

## FUSE operations status

`Lifetimefs` now implements a functional subset of `fuser::Filesystem` backed by the instance `head/` directory.

Implemented and wired to `StorageInstance` helpers:

- `lookup`
- `getattr`
- `setattr` (mode + size handling)
- `readlink`
- `mknod` (regular file creation)
- `mkdir`
- `unlink`
- `rmdir`
- `symlink`
- `rename`
- `link`
- `opendir`
- `readdir`
- `releasedir`
- `read`

Current behavior details:

- Path-based operations are parent-inode aware (not root-only).
- Directory listing (`ls`) works through `readdir`.
- File reads are served directly from materialized bytes in `head/`.
- Inode/path mapping is currently resolved by scanning under `head/` (no metadata index yet).

Still not implemented in this stage:

- `write` and the full write-path lifecycle (`open`/`create`/`flush`/`release`/`fsync` policy for persistent updates).

So today, lifecycle + storage instance management + read-oriented filesystem operations are implemented, while write-path completeness is pending.

## Planned on-disk versioning design

This section documents the agreed storage design before implementation.

### Goals

- Latest version read in `O(1)`.
- Add a new change in `O(1)` metadata operations.
- Read file at time `T` can be slower than `O(1)`.
- Keep storage smaller than full-snapshot-per-write.

### Core idea

Use a two-tier content model:

1. History tier (`data/`): immutable version history, primarily diff-based.
2. Materialized tier (`head/`): persistent latest bytes per inode for fast reads.

`head/` is treated as durable cache/state, not the canonical audit history.

### Metadata responsibilities (`metadata.db`)

At minimum, metadata tracks:

- Filesystem-level configuration, including where `data/` is rooted.
- Inodes and POSIX-like attrs (`uid`, `gid`, `mode`, times).
- Directory entries (`parent_inode`, `name`, `child_inode`) for path resolution.
- Version chain metadata:
  - current `latest_version_id` per inode
  - links to previous versions
  - history object locations in `data/`
  - materialized latest version marker for `head/` validation

Hardlinks are out of scope for now.

### Read/write behavior

1. Get latest version of file:
   - Resolve path to inode.
   - Read inode `latest_version_id`.
   - Read bytes from persistent materialized file in `head/`.
   - Validate materialized version marker matches `latest_version_id`.

2. Add change to a file:
   - Compute/store new history object in `data/` (diff-oriented).
   - Update materialized latest bytes in `head/` (atomic write via temp + rename).
   - Insert new version metadata and move inode head pointer to new version.

3. Get file at specific point in time:
   - Resolve inode from path.
   - Walk version history back to target timestamp.
   - Reconstruct content from nearest base + diffs as needed.

### Integrity and recovery rules

- Source of truth for "current version" is metadata (`latest_version_id`).
- Updates must be crash-safe and ordered so metadata and materialized bytes cannot silently diverge.
- On startup, verify materialized files against version markers/checksums.
- If mismatch is detected, rebuild materialized file from `data/` history.

### Performance notes

- Latest read stays `O(1)` while materialized state is valid.
- History lookups may be `O(k)` in number of diffs traversed.
- To avoid unbounded replay time, periodically create base snapshots or compact chains.
