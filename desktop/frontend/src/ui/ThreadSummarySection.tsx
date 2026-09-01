import { type ReactNode, useState } from "react";

const STORAGE_PREFIX = "garive.thread-summary.";

function initialOpen(sectionKey: string, defaultCollapsed: boolean) {
  try {
    const stored = window.localStorage.getItem(`${STORAGE_PREFIX}${sectionKey}`);
    return stored === null ? !defaultCollapsed : stored === "open";
  } catch { return !defaultCollapsed; }
}

export function ThreadSummarySection({ sectionKey, title, count, defaultCollapsed = false, children }: {
  sectionKey: string;
  title: string;
  count?: number;
  defaultCollapsed?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => initialOpen(sectionKey, defaultCollapsed));
  const contentId = `thread-summary-${sectionKey}`;
  const toggle = () => setOpen((current) => {
    const next = !current;
    try { window.localStorage.setItem(`${STORAGE_PREFIX}${sectionKey}`, next ? "open" : "closed"); }
    catch { /* persistence cannot block disclosure */ }
    return next;
  });
  return <section className="environment-section" data-collapsed={!open}>
    <h2 aria-label={title}><button type="button" aria-label={count === undefined ? title : `${title}, ${count}`}
      aria-expanded={open} aria-controls={contentId} onClick={toggle}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 4 4 4-4 4" /></svg>
      <span>{title}</span>{count !== undefined && <span className="environment-section-count">{count}</span>}
    </button></h2>
    <div id={contentId} className="environment-section-body" hidden={!open}>{children}</div>
  </section>;
}
