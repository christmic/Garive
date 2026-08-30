import { useEffect, useMemo, useState } from "react";
import {
  listWorkspaceEntries, type WorkspaceEntry, type WorkspaceGrant,
} from "../ipc/host";
import { Icon } from "../ui/Icon";

interface DirectoryLevel { readonly entryId?: string; readonly label: string }

const previewEntries: readonly WorkspaceEntry[] = [
  { schema_version: 1, entry_id: "entry-brief", parent_entry_id: null,
    display_name: "Launch brief.md", kind: "text", byte_size: 6842, selectable: true },
  { schema_version: 1, entry_id: "entry-notes", parent_entry_id: null,
    display_name: "Research notes", kind: "directory", byte_size: null, selectable: true },
  { schema_version: 1, entry_id: "entry-image", parent_entry_id: null,
    display_name: "Reference.png", kind: "image", byte_size: 184320, selectable: true },
];

export function WorkspacePicker({ grant, preview = false, onCancel, onConfirm }: {
  readonly grant: WorkspaceGrant;
  readonly preview?: boolean;
  readonly onCancel: () => void;
  readonly onConfirm: (entries: readonly WorkspaceEntry[]) => void;
}) {
  const [levels, setLevels] = useState<readonly DirectoryLevel[]>([
    { label: grant.display_name },
  ]);
  const [entries, setEntries] = useState<readonly WorkspaceEntry[]>([]);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const current = levels.at(-1);

  const load = async (append = false) => {
    setLoading(true); setError(undefined);
    try {
      if (preview) {
        setEntries(previewEntries); setCursor(null); return;
      }
      const page = await listWorkspaceEntries(
        grant.workspace_id, current?.entryId, append ? cursor ?? undefined : undefined,
      );
      setEntries((existing) => append ? [...existing, ...page.entries] : page.entries);
      setCursor(page.next_cursor);
    } catch {
      setError("This folder could not be read safely. Choose another Workspace or try again.");
    } finally { setLoading(false); }
  };

  useEffect(() => { void load(); }, [levels]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const close = (event: KeyboardEvent) => { if (event.key === "Escape") onCancel(); };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onCancel]);

  const chosen = useMemo(
    () => entries.filter((entry) => selected.has(entry.entry_id)), [entries, selected],
  );
  const enter = (entry: WorkspaceEntry) => {
    if (entry.kind !== "directory" || !entry.selectable) return;
    setLevels((value) => [...value, { entryId: entry.entry_id, label: entry.display_name }]);
    setEntries([]); setCursor(null); setSelected(new Set());
  };
  const back = () => {
    if (levels.length < 2) return;
    setLevels((value) => value.slice(0, -1));
    setEntries([]); setCursor(null); setSelected(new Set());
  };
  const toggle = (entry: WorkspaceEntry) => setSelected((value) => {
    const next = new Set(value);
    if (next.has(entry.entry_id)) next.delete(entry.entry_id);
    else if (next.size < 8) next.add(entry.entry_id);
    return next;
  });

  return <div className="workspace-scrim" role="presentation" onMouseDown={(event) => {
    if (event.target === event.currentTarget) onCancel();
  }}>
    <section className="workspace-sheet" role="dialog" aria-modal="true"
      aria-labelledby="workspace-title">
      <header>
        <div className="workspace-heading"><span><Icon name="work" /></span><div>
          <p className="eyebrow">ADD LOCAL CONTEXT</p>
          <h2 id="workspace-title">Choose files from {grant.display_name}</h2>
        </div></div>
        <button className="icon-button" type="button" aria-label="Close file picker"
          autoFocus onClick={onCancel}><Icon name="close" /></button>
      </header>
      <div className="workspace-path">
        <button type="button" disabled={levels.length < 2} onClick={back}
          aria-label="Back to parent folder">‹</button>
        <span title={current?.label}>{current?.label}</span>
        <small>{selected.size}/8 selected</small>
      </div>
      <div className="workspace-list" aria-busy={loading}>
        {loading && !entries.length ? <div className="workspace-list-state"><span className="spinner" />Reading safe metadata…</div>
          : error ? <div className="workspace-list-state error"><Icon name="warning" />{error}<button type="button" onClick={() => void load()}>Retry</button></div>
            : entries.length ? entries.map((entry) => {
              const directory = entry.kind === "directory";
              const selectable = entry.selectable && entry.kind === "text";
              return <div className={`workspace-entry ${!entry.selectable ? "blocked" : ""}`}
                key={entry.entry_id}>
                <button className="entry-main" type="button"
                  disabled={!directory || !entry.selectable} onClick={() => enter(entry)}>
                  <span className="entry-icon"><Icon name={directory ? "archive" : "file"} /></span>
                  <span><strong dir="auto">{entry.display_name}</strong><small>{entryCopy(entry)}</small></span>
                  {directory && entry.selectable && <Icon name="chevron" />}
                </button>
                {selectable && <label className="entry-check"><input type="checkbox"
                  checked={selected.has(entry.entry_id)} onChange={() => toggle(entry)}
                  aria-label={`Select ${entry.display_name}`} /><span /></label>}
              </div>;
            }) : <div className="workspace-list-state"><Icon name="file" />No eligible items in this folder.</div>}
        {cursor && !loading && <button className="workspace-more" type="button"
          onClick={() => void load(true)}>Load more</button>}
      </div>
      <footer><p><Icon name="shield" />Only selected UTF-8 text goes directly from the Rust backend to Runtime.</p>
        <div><button className="secondary-button" type="button" onClick={onCancel}>Cancel</button>
          <button className="primary-button" type="button" disabled={!chosen.length}
            onClick={() => onConfirm(chosen)}>Add {chosen.length || ""} {chosen.length === 1 ? "file" : "files"}</button></div>
      </footer>
    </section>
  </div>;
}

function entryCopy(entry: WorkspaceEntry) {
  if (!entry.selectable) return "Protected package";
  if (entry.kind === "directory") return "Folder";
  const size = entry.byte_size == null ? "" : ` · ${formatBytes(entry.byte_size)}`;
  return `${entry.kind === "text" ? "Text" : "Preview only"}${size}`;
}
function formatBytes(bytes: number) {
  return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024
    ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
