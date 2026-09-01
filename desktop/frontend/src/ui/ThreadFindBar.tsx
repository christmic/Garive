import { useEffect, useRef, useState, type RefObject } from "react";
import type { MessageKey } from "../i18n";
import { Icon } from "./Icon";
import { clearThreadFindMatches, findThreadTextMatches } from "./threadFind";

const RESULT_LIMIT = 500;

export function ThreadFindBar({ open, openRevision, container, onClose, t }: {
  readonly open: boolean;
  readonly openRevision: number;
  readonly container: RefObject<HTMLElement | null>;
  readonly onClose: () => void;
  readonly t: (key: MessageKey) => string;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<number | null>(null);
  const [result, setResult] = useState({ count: 0, capped: false });
  const [searching, setSearching] = useState(false);
  const [contentRevision, setContentRevision] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const matches = useRef<readonly HTMLElement[]>([]);
  const observer = useRef<MutationObserver | undefined>(undefined);
  const timer = useRef<number | undefined>(undefined);
  const returnFocus = useRef<HTMLElement | null>(null);

  const observe = () => {
    const root = container.current;
    if (root && observer.current) observer.current.observe(root,
      { childList: true, characterData: true, subtree: true });
  };
  const clear = (reobserve = true) => {
    const root = container.current;
    observer.current?.disconnect();
    if (root) clearThreadFindMatches(root);
    matches.current = [];
    if (reobserve) observe();
  };
  const runSearch = (selectFirst: boolean) => {
    if (timer.current !== undefined) window.clearTimeout(timer.current);
    clear(false);
    const root = container.current;
    const trimmed = query.trim();
    if (!root || !trimmed) {
      setSearching(false); setResult({ count: 0, capped: false }); setActive(null); observe(); return;
    }
    const next = findThreadTextMatches(root, trimmed, RESULT_LIMIT);
    matches.current = next.matches;
    observe();
    setSearching(false); setResult({ count: next.matches.length, capped: next.capped });
    setActive((current) => selectFirst ? next.matches.length ? 0 : null
      : current != null && current < next.matches.length ? current : next.matches.length ? 0 : null);
  };
  const close = () => {
    setQuery(""); setResult({ count: 0, capped: false }); setActive(null); clear(); onClose();
    requestAnimationFrame(() => returnFocus.current?.isConnected && returnFocus.current.focus());
  };
  const navigate = (direction: 1 | -1) => {
    if (!query.trim()) return;
    if (!matches.current.length) { runSearch(true); return; }
    setActive((current) => ((current ?? (direction > 0 ? -1 : 0))
      + direction + matches.current.length) % matches.current.length);
  };

  useEffect(() => {
    if (!open) return;
    observer.current = new MutationObserver((records) => {
      if (records.some((record) => !searchOnlyMutation(record))) {
        setContentRevision((revision) => revision + 1);
      }
    });
    observe();
    return () => { observer.current?.disconnect(); observer.current = undefined; };
  }, [open, container]);

  useEffect(() => {
    if (!open) return;
    setSearching(Boolean(query.trim()));
    timer.current = window.setTimeout(() => runSearch(false), 150);
    return () => {
      if (timer.current !== undefined) window.clearTimeout(timer.current);
      clear();
    };
  }, [contentRevision, open, query]);

  useEffect(() => {
    if (!open) return;
    if (document.activeElement !== input.current && document.activeElement instanceof HTMLElement) {
      returnFocus.current = document.activeElement;
    }
    const selection = window.getSelection?.()?.toString().trim();
    if (selection && !/[\r\n]/.test(selection)) setQuery(selection);
    requestAnimationFrame(() => { input.current?.focus(); input.current?.select(); });
  }, [open, openRevision]);

  useEffect(() => {
    matches.current.forEach((match, index) => match.toggleAttribute("data-active", index === active));
    const match = active == null ? undefined : matches.current[active];
    match?.scrollIntoView?.({ behavior: "auto", block: "center" });
  }, [active, result]);

  if (!open) return null;
  const hasQuery = Boolean(query.trim());
  const label = !hasQuery ? "" : result.count === 0 ? t("thread.findNoResults")
    : t(result.capped ? "thread.findResultsCapped" : "thread.findResults")
      .replace("{active}", String((active ?? 0) + 1)).replace("{matches}", String(result.count));
  return <div className="thread-find-layer" data-thread-find-skip="">
    <div className="thread-find-surface" role="search">
      <div className="thread-find-input-row">
        <Icon name="search" className={searching ? "searching" : undefined} />
        <label className="sr-only" htmlFor="thread-find-input">{t("thread.find")}</label>
        <input ref={input} id="thread-find-input" type="text" value={query}
          aria-busy={searching || undefined} aria-label={t("thread.find")}
          placeholder={t("thread.findPlaceholder")} onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") { event.preventDefault(); navigate(event.shiftKey ? -1 : 1); }
            if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); }
          }} />
      </div>
      <div className={hasQuery ? "thread-find-navigation visible" : "thread-find-navigation"}>
        <button type="button" disabled={!result.count} aria-label={t("thread.findPrevious")}
          onClick={() => navigate(-1)}><Icon name="chevron" /></button>
        <button type="button" disabled={!result.count} aria-label={t("thread.findNext")}
          onClick={() => navigate(1)}><Icon name="chevron" /></button>
      </div>
      <span className={hasQuery ? "thread-find-result visible" : "thread-find-result"}
        aria-live="polite">{label}</span>
      <div className="thread-find-close"><span aria-hidden="true" />
        <button type="button" aria-label={t("thread.findClose")} onClick={close}>
          <Icon name="close" /></button>
      </div>
    </div>
  </div>;
}

function searchOnlyMutation(record: MutationRecord): boolean {
  if (record.type !== "childList") return false;
  if (record.target instanceof HTMLElement && record.target.matches("mark[data-search-match]")) return true;
  return [...record.addedNodes, ...record.removedNodes].every((node) => node instanceof Text
    || node instanceof HTMLElement && node.matches("mark[data-search-match]"));
}
