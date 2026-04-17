# Local History

Local history records snapshots of files in your workspace when you save. It
lets you review changes and restore earlier versions from the Local History
panel.

## Getting Started {#getting-started}

1. Open the Local History panel from the status bar or the command bar.
2. Save a file.
3. Select an entry to review changes or restore it.

## Local History Panel {#local-history-panel}

The panel shows entries for the active editor file. Each entry includes a
timestamp and actions to open a diff, bookmark the entry, or restore that
snapshot.

Bookmarked entries are outlined with the bookmark accent color so important
recovery points remain easy to spot.

Right-click an entry to open a reconstruction view. That view keeps the diff
open against your current file, adds per-hunk restore controls, and lets you
restore either selected hunks or the full snapshot.

Local history reads from all configured storage endpoints. New snapshots are
written to the active endpoint.

## CLI {#cli}

`zed-herodotus` is a one-shot CLI for the same local-history store used by the
panel. It is useful for scripts and AI agents that need to inspect or restore
history without driving the UI.

Examples:

```sh
zed-herodotus list --worktree /path/to/project --file src/main.rs
zed-herodotus hunks --worktree /path/to/project --file src/main.rs --entry 1700000000-42 --json
zed-herodotus diff --worktree /path/to/project --file src/main.rs --entry 1700000000-42
zed-herodotus bookmark --worktree /path/to/project --file src/main.rs --entry 1700000000-42 --mode toggle
zed-herodotus restore --worktree /path/to/project --file src/main.rs --entry 1700000000-42 --hunk 0 --hunk 2 --output /tmp/reconstructed.rs
zed-herodotus restore --worktree /path/to/project --file src/main.rs --entry 1700000000-42 --in-place
```

Pass `--root` multiple times to search custom local-history endpoints. If no
root is supplied, the CLI falls back to Zed's default local-history data
directory.

## Storage and Retention {#storage-and-retention}

Local history is stored on your machine. By default, it uses Zed's data
directory, but you can add additional storage endpoints and switch the active
one (for example, to an external drive).

Snapshots are excluded by default for common, easily regenerated paths and
file types, such as `node_modules` and `*.min.*`. Excluded paths are not
recorded and do not count toward size limits.

Retention defaults:

- Per-worktree cap: 0.12% of available free space in the active endpoint, or
  300 MiB (whichever is larger)
- Minimum age before deletion: 100 days
- Pruning policy: `both` (entries are removed only when the cap is exceeded
  and the entry is older than the minimum age)

## Settings {#settings}

Use the Settings Editor to configure the Local History panel button, dock, and
default width.

Or add this to your `settings.json`:

```json [settings]
{
  "local_history": {
    "enabled": true,
    "capture_on_save": true,
    "storage_paths": ["/Volumes/External/ZedHistory"],
    "active_storage_path": "/Volumes/External/ZedHistory",
    "min_age_days": 100,
    "cap_free_space_percent": 0.12,
    "cap_min_bytes": 314572800,
    "prune_policy": "both",
    "exclude_globs": [
      "**/.git/**",
      "**/.hg/**",
      "**/.svn/**",
      "**/.jj/**",
      "**/node_modules/**",
      "**/target/**",
      "**/dist/**",
      "**/build/**",
      "**/out/**",
      "**/.gradle/**",
      "**/.idea/**",
      "**/.zed/**",
      "**/*.min.*",
      "**/*.map"
    ]
  },
  "local_history_panel": {
    "button": true,
    "dock": "right",
    "default_width": 300,
    "show_relative_path": false
  }
}
```

Valid `prune_policy` values are `both`, `size_only`, `age_only`, and `any`.
