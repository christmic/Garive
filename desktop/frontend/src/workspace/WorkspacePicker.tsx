import { useEffect, useMemo, useRef, useState } from "react";
import {
  listWorkspaceEntries, type WorkspaceEntry, type WorkspaceGrant,
} from "../ipc/host";
import { Icon } from "../ui/Icon";
import { createTranslator, type Translator } from "../i18n";

interface DirectoryLevel { readonly entryId?: string; readonly label: string }

const previewEntries: readonly WorkspaceEntry[] = [
  { schema_version: 1, entry_id: "entry-brief", parent_entry_id: null,
    display_name: "Launch brief.md", kind: "text", byte_size: 6842, selectable: true },
  { schema_version: 1, entry_id: "entry-notes", parent_entry_id: null,
    display_name: "Research notes", kind: "directory", byte_size: null, selectable: true },
  { schema_version: 1, entry_id: "entry-image", parent_entry_id: null,
    display_name: "Reference.png", kind: "image", byte_size: 184320, selectable: true },
];

export function WorkspacePicker({ grant, preview = false, onCancel, onConfirm,
  t = createTranslator("en") }: {
  readonly grant: WorkspaceGrant;
  readonly preview?: boolean;
  readonly onCancel: () => void;
  readonly onConfirm: (entries: readonly WorkspaceEntry[]) => void;
  readonly t?: Translator;
}) {
  const [levels, setLevels] = useState<readonly DirectoryLevel[]>([
    { label: grant.display_name },
  ]);
  const [entries, setEntries] = useState<readonly WorkspaceEntry[]>([]);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const sheet = useRef<HTMLElement>(null);
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
      setError(t("workspace.readError"));
    } finally { setLoading(false); }
  };

  useEffect(() => { void load(); }, [levels]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const containFocus = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onCancel(); return; }
      if (event.key !== "Tab") return;
      const controls = [...(sheet.current?.querySelectorAll<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex='-1'])",
      ) ?? [])];
      const first = controls[0]; const last = controls.at(-1);
      if (!first || !last) return;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault(); last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault(); first.focus();
      }
    };
    window.addEventListener("keydown", containFocus);
    return () => window.removeEventListener("keydown", containFocus);
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
    <section ref={sheet} className="workspace-sheet" role="dialog" aria-modal="true"
      aria-labelledby="workspace-title">
      <header>
        <div className="workspace-heading"><span><Icon name="work" /></span><div>
          <p className="eyebrow">{t("workspace.eyebrow")}</p>
          <h2 id="workspace-title">{t("workspace.choosePrefix")} <bdi>{grant.display_name}</bdi>{t("workspace.chooseSuffix")}</h2>
        </div></div>
        <button className="icon-button" type="button" aria-label={t("workspace.close")}
          autoFocus onClick={onCancel}><Icon name="close" /></button>
      </header>
      <div className="workspace-path">
        <button type="button" disabled={levels.length < 2} onClick={back}
          aria-label={t("workspace.back")}>‹</button>
        <span title={current?.label}>{current?.label}</span>
        <small>{selected.size}/8 {t("workspace.selected")}</small>
      </div>
      <div className="workspace-list" aria-label={t("workspace.files")} aria-busy={loading}>
        {loading && !entries.length ? <div className="workspace-list-state"><span className="spinner" />{t("workspace.reading")}</div>
          : error ? <div className="workspace-list-state error"><Icon name="warning" />{error}<button type="button" onClick={() => void load()}>{t("workspace.retry")}</button></div>
            : entries.length ? entries.map((entry) => {
              const directory = entry.kind === "directory";
              const selectable = entry.selectable && entry.kind === "text";
              return <div className={`workspace-entry ${!entry.selectable ? "blocked" : ""}`}
                key={entry.entry_id}>
                <button className="entry-main" type="button"
                  disabled={!directory || !entry.selectable} onClick={() => enter(entry)}>
                  <span className="entry-icon"><Icon name={directory ? "archive" : "file"} /></span>
                  <span><strong dir="auto">{entry.display_name}</strong><small>{entryCopy(entry, t)}</small></span>
                  {directory && entry.selectable && <Icon name="chevron" />}
                </button>
                {selectable && <label className="entry-check"><input type="checkbox"
                  checked={selected.has(entry.entry_id)} onChange={() => toggle(entry)}
                  aria-label={t("workspace.select")} /><span /></label>}
              </div>;
            }) : <div className="workspace-list-state"><Icon name="file" />{t("workspace.empty")}</div>}
        {cursor && !loading && <button className="workspace-more" type="button"
          onClick={() => void load(true)}>{t("workspace.more")}</button>}
      </div>
      <footer><p><Icon name="shield" />{t("workspace.safety")}</p>
        <div><button className="secondary-button" type="button" onClick={onCancel}>{t("workspace.cancel")}</button>
          <button className="primary-button" type="button" disabled={!chosen.length}
            onClick={() => onConfirm(chosen)}>{t("workspace.add")} {chosen.length || ""} {t(chosen.length === 1 ? "workspace.file" : "workspace.filesPlural")}</button></div>
      </footer>
    </section>
  </div>;
}

function entryCopy(entry: WorkspaceEntry, t: Translator) {
  if (!entry.selectable) return t("workspace.package");
  if (entry.kind === "directory") return t("workspace.folder");
  const size = entry.byte_size == null ? "" : ` · ${formatBytes(entry.byte_size)}`;
  return `${t(entry.kind === "text" ? "workspace.text" : "workspace.previewOnly")}${size}`;
}
function formatBytes(bytes: number) {
  return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024
    ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
