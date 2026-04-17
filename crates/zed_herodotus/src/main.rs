use anyhow::{Context as _, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use language::{apply_diff_patch, line_diff, unified_diff, unified_diff_with_offsets};
use project::{
    LocalHistoryEntry, LocalHistorySettings, LocalHistoryStorageKind, LocalHistoryTransferMode,
    load_local_history_entries, load_local_history_entry_text, prune_local_history_worktree,
    set_local_history_entry_bookmarked, transfer_local_history,
};
use serde::Serialize;
use settings::LocalHistoryPrunePolicy;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "zed-herodotus",
    about = "Inspect and manipulate Zed local-history snapshots"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    List(ListCommand),
    Show(ShowCommand),
    Diff(DiffCommand),
    Hunks(HunksCommand),
    Restore(RestoreCommand),
    Bookmark(BookmarkCommand),
    Prune(PruneCommand),
    Transfer(TransferCommand),
}

#[derive(Args, Debug, Clone)]
struct TargetArgs {
    /// The worktree root used when the history entry was captured.
    #[arg(long)]
    worktree: PathBuf,

    /// The file path within the worktree. Absolute paths are accepted if they live under `--worktree`.
    #[arg(long)]
    file: PathBuf,

    /// Local-history root directories. Repeat to search multiple endpoints.
    #[arg(long = "root")]
    roots: Vec<PathBuf>,
}

#[derive(Args, Debug)]
struct ListCommand {
    #[command(flatten)]
    target: TargetArgs,

    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct ShowCommand {
    #[command(flatten)]
    target: TargetArgs,

    /// Entry id from `list`.
    #[arg(long)]
    entry: String,

    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct DiffCommand {
    #[command(flatten)]
    target: TargetArgs,

    /// Entry id from `list`.
    #[arg(long)]
    entry: String,

    /// File to diff against. Defaults to `--file`.
    #[arg(long)]
    against: Option<PathBuf>,

    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct HunksCommand {
    #[command(flatten)]
    target: TargetArgs,

    /// Entry id from `list`.
    #[arg(long)]
    entry: String,

    /// File to reconstruct against. Defaults to `--file`.
    #[arg(long)]
    against: Option<PathBuf>,

    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct RestoreCommand {
    #[command(flatten)]
    target: TargetArgs,

    /// Entry id from `list`.
    #[arg(long)]
    entry: String,

    /// Restore directly into the target file.
    #[arg(long, conflicts_with = "output")]
    in_place: bool,

    /// Write the snapshot to a different output path.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Restore only the selected hunk indexes from `hunks`.
    #[arg(long = "hunk")]
    hunks: Vec<usize>,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum BookmarkMode {
    Set,
    Unset,
    Toggle,
}

#[derive(Args, Debug)]
struct BookmarkCommand {
    #[command(flatten)]
    target: TargetArgs,

    /// Entry id from `list`.
    #[arg(long)]
    entry: String,

    #[arg(long, value_enum, default_value_t = BookmarkMode::Toggle)]
    mode: BookmarkMode,

    #[arg(long)]
    json: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum PrunePolicyArg {
    Both,
    SizeOnly,
    AgeOnly,
    Any,
}

#[derive(Args, Debug)]
struct PruneCommand {
    /// Local-history root directory. Defaults to Zed's standard local-history data dir.
    #[arg(long)]
    root: Option<PathBuf>,

    /// The worktree root whose history should be pruned.
    #[arg(long)]
    worktree: PathBuf,

    #[arg(long)]
    min_age_days: Option<u64>,

    #[arg(long)]
    cap_free_space_percent: Option<f32>,

    #[arg(long)]
    cap_min_bytes: Option<u64>,

    #[arg(long, value_enum)]
    prune_policy: Option<PrunePolicyArg>,

    #[arg(long)]
    json: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum TransferModeArg {
    Copy,
    Move,
}

#[derive(Args, Debug)]
struct TransferCommand {
    #[arg(long)]
    source: PathBuf,

    #[arg(long)]
    destination: PathBuf,

    #[arg(long, value_enum, default_value_t = TransferModeArg::Copy)]
    mode: TransferModeArg,
}

#[derive(Debug)]
struct ResolvedTarget {
    worktree: PathBuf,
    file_path: PathBuf,
    relative_path: String,
    roots: Vec<PathBuf>,
}

#[derive(Serialize)]
struct EntryOutput {
    id: String,
    timestamp: String,
    relative_path: String,
    endpoint_root: String,
    snapshot_path: String,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
    bookmarked: bool,
    storage_kind: &'static str,
    base_entry_id: Option<String>,
    chain_depth: u16,
}

#[derive(Serialize)]
struct ShowOutput {
    entry: EntryOutput,
    text: String,
}

#[derive(Serialize)]
struct DiffOutput {
    entry: EntryOutput,
    against: String,
    diff: String,
}

#[derive(Serialize)]
struct HunkOutput {
    index: usize,
    kind: &'static str,
    current_start_line: u32,
    current_line_count: u32,
    snapshot_start_line: u32,
    snapshot_line_count: u32,
    patch: String,
}

#[derive(Serialize)]
struct HunksOutput {
    entry: EntryOutput,
    against: String,
    hunks: Vec<HunkOutput>,
}

#[derive(Serialize)]
struct BookmarkOutput {
    entry: EntryOutput,
    bookmarked: bool,
}

#[derive(Serialize)]
struct PruneOutput {
    root: String,
    worktree: String,
    removed_entries: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List(command) => list_command(command),
        Command::Show(command) => show_command(command),
        Command::Diff(command) => diff_command(command),
        Command::Hunks(command) => hunks_command(command),
        Command::Restore(command) => restore_command(command),
        Command::Bookmark(command) => bookmark_command(command),
        Command::Prune(command) => prune_command(command),
        Command::Transfer(command) => transfer_command(command),
    }
}

fn list_command(command: ListCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let entries = entries_for_target(&target);

    if command.json {
        print_json(
            &entries
                .iter()
                .map(entry_to_output)
                .collect::<Vec<EntryOutput>>(),
        )?;
        return Ok(());
    }

    if entries.is_empty() {
        println!("No local-history entries found.");
        return Ok(());
    }

    for entry in entries {
        let bookmark = if entry.bookmarked { "*" } else { " " };
        println!(
            "{bookmark} {}\t{}\t{}\t{}\t{}",
            entry.id,
            entry.timestamp,
            storage_kind_name(entry.storage_kind),
            byte_summary(&entry),
            entry.relative_path,
        );
    }
    Ok(())
}

fn show_command(command: ShowCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let entry = find_entry(&target, &command.entry)?;
    let text = load_local_history_entry_text(&entry)
        .with_context(|| format!("loading local-history entry {}", entry.id))?;

    if command.json {
        print_json(&ShowOutput {
            entry: entry_to_output(&entry),
            text: text.to_string(),
        })?;
        return Ok(());
    }

    print!("{text}");
    Ok(())
}

fn diff_command(command: DiffCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let entry = find_entry(&target, &command.entry)?;
    let snapshot_text = load_local_history_entry_text(&entry)
        .with_context(|| format!("loading local-history entry {}", entry.id))?;
    let against_path = command.against.unwrap_or_else(|| target.file_path.clone());
    let against_text =
        fs::read_to_string(&against_path).with_context(|| format!("reading {:?}", against_path))?;
    let diff_body = unified_diff(snapshot_text.as_ref(), &against_text);
    let against_label = against_path.to_string_lossy().into_owned();

    if command.json {
        print_json(&DiffOutput {
            entry: entry_to_output(&entry),
            against: against_label,
            diff: diff_body,
        })?;
        return Ok(());
    }

    if diff_body.is_empty() {
        println!("No diff.");
        return Ok(());
    }

    println!("--- a/{}", target.relative_path);
    println!("+++ b/{against_label}");
    print!("{diff_body}");
    Ok(())
}

fn hunks_command(command: HunksCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let entry = find_entry(&target, &command.entry)?;
    let snapshot_text = load_local_history_entry_text(&entry)
        .with_context(|| format!("loading local-history entry {}", entry.id))?;
    let against_path = command.against.unwrap_or_else(|| target.file_path.clone());
    let against_text =
        fs::read_to_string(&against_path).with_context(|| format!("reading {:?}", against_path))?;
    let against_label = against_path.to_string_lossy().into_owned();
    let hunks = compute_restore_hunks(&against_text, snapshot_text.as_ref());

    if command.json {
        print_json(&HunksOutput {
            entry: entry_to_output(&entry),
            against: against_label,
            hunks,
        })?;
        return Ok(());
    }

    if hunks.is_empty() {
        println!("No restore hunks.");
        return Ok(());
    }

    for hunk in hunks {
        println!(
            "[{}] {}\tcurrent:{}+{}\tsnapshot:{}+{}",
            hunk.index,
            hunk.kind,
            hunk.current_start_line,
            hunk.current_line_count,
            hunk.snapshot_start_line,
            hunk.snapshot_line_count
        );
    }
    Ok(())
}

fn restore_command(command: RestoreCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let destination = match (command.in_place, command.output) {
        (true, None) => target.file_path.clone(),
        (false, Some(path)) => path,
        (false, None) => {
            bail!("restore needs `--in-place` or `--output <path>`");
        }
        (true, Some(_)) => unreachable!("clap enforces conflicts"),
    };

    let entry = find_entry(&target, &command.entry)?;
    let snapshot_text = load_local_history_entry_text(&entry)
        .with_context(|| format!("loading local-history entry {}", entry.id))?;
    let (restored_text, selected_hunk_count) = if command.hunks.is_empty() {
        (snapshot_text.to_string(), None)
    } else {
        let current_text = fs::read_to_string(&target.file_path)
            .with_context(|| format!("reading {:?}", target.file_path))?;
        let hunks = compute_restore_hunks(&current_text, snapshot_text.as_ref());
        let indexes = normalize_hunk_indexes(&command.hunks, hunks.len())?;
        let patch = indexes
            .iter()
            .map(|ix| hunks[*ix].patch.as_str())
            .collect::<String>();
        (
            apply_diff_patch(&current_text, &patch).context("applying selected restore hunks")?,
            Some(indexes.len()),
        )
    };

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory for {:?}", destination))?;
    }
    fs::write(&destination, restored_text).with_context(|| format!("writing {:?}", destination))?;

    if let Some(selected_hunk_count) = selected_hunk_count {
        println!(
            "Restored {} selected hunks from {} to {}",
            selected_hunk_count,
            entry.id,
            destination.to_string_lossy()
        );
    } else {
        println!("Restored {} to {}", entry.id, destination.to_string_lossy());
    }
    Ok(())
}

fn bookmark_command(command: BookmarkCommand) -> Result<()> {
    let target = resolve_target(&command.target)?;
    let entry = find_entry(&target, &command.entry)?;
    let bookmarked = match command.mode {
        BookmarkMode::Set => true,
        BookmarkMode::Unset => false,
        BookmarkMode::Toggle => !entry.bookmarked,
    };
    set_local_history_entry_bookmarked(&entry, bookmarked)
        .with_context(|| format!("updating bookmark for {}", entry.id))?;

    let mut updated_entry = entry.clone();
    updated_entry.bookmarked = bookmarked;

    if command.json {
        print_json(&BookmarkOutput {
            entry: entry_to_output(&updated_entry),
            bookmarked,
        })?;
        return Ok(());
    }

    println!(
        "{} {}",
        if bookmarked {
            "Bookmarked"
        } else {
            "Unbookmarked"
        },
        updated_entry.id
    );
    Ok(())
}

fn prune_command(command: PruneCommand) -> Result<()> {
    let root = command
        .root
        .unwrap_or_else(|| LocalHistorySettings::default().resolved_active_path());
    let worktree = canonicalize_existing_path(&command.worktree)?;

    let mut settings = LocalHistorySettings::default();
    settings.active_storage_path = Some(root.to_string_lossy().into_owned());
    settings.storage_paths = vec![root.to_string_lossy().into_owned()];
    if let Some(min_age_days) = command.min_age_days {
        settings.min_age_days = min_age_days;
    }
    if let Some(cap_free_space_percent) = command.cap_free_space_percent {
        settings.cap_free_space_percent = cap_free_space_percent;
    }
    if let Some(cap_min_bytes) = command.cap_min_bytes {
        settings.cap_min_bytes = cap_min_bytes;
    }
    if let Some(prune_policy) = command.prune_policy {
        settings.prune_policy = map_prune_policy(prune_policy);
    }

    let removed = prune_local_history_worktree(&root, &worktree, &settings)?;

    if command.json {
        print_json(&PruneOutput {
            root: root.to_string_lossy().into_owned(),
            worktree: worktree.to_string_lossy().into_owned(),
            removed_entries: removed,
        })?;
        return Ok(());
    }

    println!("Removed {removed} entries");
    Ok(())
}

fn transfer_command(command: TransferCommand) -> Result<()> {
    transfer_local_history(
        &command.source,
        &command.destination,
        match command.mode {
            TransferModeArg::Copy => LocalHistoryTransferMode::Copy,
            TransferModeArg::Move => LocalHistoryTransferMode::Move,
        },
    )?;
    println!(
        "{} local history from {} to {}",
        match command.mode {
            TransferModeArg::Copy => "Copied",
            TransferModeArg::Move => "Moved",
        },
        command.source.to_string_lossy(),
        command.destination.to_string_lossy()
    );
    Ok(())
}

fn resolve_target(args: &TargetArgs) -> Result<ResolvedTarget> {
    let worktree = canonicalize_existing_path(&args.worktree)?;
    let file_path = resolve_file_path(&worktree, &args.file)?;
    let relative_path = file_path
        .strip_prefix(&worktree)
        .with_context(|| {
            format!(
                "file {:?} must be inside worktree {:?}",
                file_path, worktree
            )
        })?
        .to_string_lossy()
        .into_owned();

    Ok(ResolvedTarget {
        worktree,
        file_path,
        relative_path,
        roots: resolve_roots(&args.roots),
    })
}

fn resolve_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    if roots.is_empty() {
        LocalHistorySettings::default().resolved_storage_paths()
    } else {
        roots.to_vec()
    }
}

fn resolve_file_path(worktree: &Path, file: &Path) -> Result<PathBuf> {
    let joined = if file.is_absolute() {
        file.to_path_buf()
    } else {
        worktree.join(file)
    };
    let joined_for_error = joined.clone();

    joined
        .canonicalize()
        .or_else(|_| {
            if joined.starts_with(worktree) {
                Ok(joined.clone())
            } else {
                Err(anyhow!("file {:?} is outside the worktree", joined))
            }
        })
        .with_context(|| format!("resolving {:?}", joined_for_error))
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("resolving {:?}", path))
}

fn entries_for_target(target: &ResolvedTarget) -> Vec<LocalHistoryEntry> {
    load_local_history_entries(
        target.roots.clone(),
        &target.worktree,
        &target.relative_path,
    )
}

fn find_entry(target: &ResolvedTarget, entry_id: &str) -> Result<LocalHistoryEntry> {
    entries_for_target(target)
        .into_iter()
        .find(|entry| entry.id == entry_id)
        .with_context(|| format!("local-history entry `{entry_id}` was not found"))
}

fn storage_kind_name(kind: LocalHistoryStorageKind) -> &'static str {
    match kind {
        LocalHistoryStorageKind::Snapshot => "snapshot",
        LocalHistoryStorageKind::Delta => "delta",
    }
}

fn compute_restore_hunks(current_text: &str, snapshot_text: &str) -> Vec<HunkOutput> {
    let current_lines = split_lines(current_text);
    let snapshot_lines = split_lines(snapshot_text);

    line_diff(current_text, snapshot_text)
        .into_iter()
        .enumerate()
        .map(|(index, (current_range, snapshot_range))| {
            let current_chunk = join_line_range(&current_lines, &current_range);
            let snapshot_chunk = join_line_range(&snapshot_lines, &snapshot_range);
            HunkOutput {
                index,
                kind: hunk_kind(&current_range, &snapshot_range),
                current_start_line: current_range.start + 1,
                current_line_count: current_range.end - current_range.start,
                snapshot_start_line: snapshot_range.start + 1,
                snapshot_line_count: snapshot_range.end - snapshot_range.start,
                patch: unified_diff_with_offsets(
                    &current_chunk,
                    &snapshot_chunk,
                    current_range.start,
                    snapshot_range.start,
                ),
            }
        })
        .collect()
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn join_line_range(lines: &[&str], range: &Range<u32>) -> String {
    lines[range.start as usize..range.end as usize].concat()
}

fn hunk_kind(current_range: &Range<u32>, snapshot_range: &Range<u32>) -> &'static str {
    if current_range.is_empty() {
        "added"
    } else if snapshot_range.is_empty() {
        "deleted"
    } else {
        "modified"
    }
}

fn normalize_hunk_indexes(indexes: &[usize], hunk_count: usize) -> Result<Vec<usize>> {
    let mut normalized = indexes.to_vec();
    normalized.sort_unstable();
    normalized.dedup();

    let Some(invalid) = normalized.iter().find(|ix| **ix >= hunk_count) else {
        return Ok(normalized);
    };

    let max_index = hunk_count.saturating_sub(1);
    bail!("hunk index {invalid} is out of range 0..={max_index}");
}

fn byte_summary(entry: &LocalHistoryEntry) -> String {
    format!("{}/{}", entry.compressed_bytes, entry.uncompressed_bytes)
}

fn entry_to_output(entry: &LocalHistoryEntry) -> EntryOutput {
    EntryOutput {
        id: entry.id.clone(),
        timestamp: entry.timestamp.to_string(),
        relative_path: entry.relative_path.to_string(),
        endpoint_root: entry.endpoint_root.to_string_lossy().into_owned(),
        snapshot_path: entry.snapshot_path().to_string_lossy().into_owned(),
        compressed_bytes: entry.compressed_bytes,
        uncompressed_bytes: entry.uncompressed_bytes,
        bookmarked: entry.bookmarked,
        storage_kind: storage_kind_name(entry.storage_kind),
        base_entry_id: entry.base_entry_id.clone(),
        chain_depth: entry.chain_depth,
    }
}

fn map_prune_policy(policy: PrunePolicyArg) -> LocalHistoryPrunePolicy {
    match policy {
        PrunePolicyArg::Both => LocalHistoryPrunePolicy::Both,
        PrunePolicyArg::SizeOnly => LocalHistoryPrunePolicy::SizeOnly,
        PrunePolicyArg::AgeOnly => LocalHistoryPrunePolicy::AgeOnly,
        PrunePolicyArg::Any => LocalHistoryPrunePolicy::Any,
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selective_restore_hunks_apply_cleanly() {
        let current = "one\ntwo\nthree\n";
        let snapshot = "ONE\ntwo\nTHREE\n";

        let hunks = compute_restore_hunks(current, snapshot);
        assert_eq!(hunks.len(), 2);

        let patch = normalize_hunk_indexes(&[1, 0, 1], hunks.len())
            .unwrap()
            .into_iter()
            .map(|ix| hunks[ix].patch.as_str())
            .collect::<String>();

        let restored = apply_diff_patch(current, &patch).unwrap();
        assert_eq!(restored, snapshot);
    }

    #[test]
    fn invalid_hunk_index_errors() {
        let err = normalize_hunk_indexes(&[2], 2).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }
}
