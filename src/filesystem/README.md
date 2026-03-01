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
- `tmp/`
- `config.json`

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

`Lifetimefs` implements the `fuser::Filesystem` trait, but file operations are currently stubs (no logic yet):

- `lookup`
- `getattr`
- `setattr`
- `readlink`
- `mknod`
- `mkdir`
- `unlink`
- `rmdir`
- `symlink`
- `rename`
- `link`

So today, the main implemented behavior is mount session lifecycle plus persistent per-mountpoint storage metadata.
