import { useId, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent,
  type CSSProperties, type RefObject } from "react";
import type { MessageKey } from "../i18n";

export interface ThreadUserNavigationItem {
  readonly id: string;
  readonly text: string;
}

export function ThreadUserNavigationRail({ items, scrollElement, onNavigate, t }: {
  readonly items: readonly ThreadUserNavigationItem[];
  readonly scrollElement: RefObject<HTMLElement | null>;
  readonly onNavigate: (id: string, behavior: ScrollBehavior) => void;
  readonly t: (key: MessageKey) => string;
}) {
  const latestId = items.at(-1)?.id;
  const [currentIds, setCurrentIds] = useState<ReadonlySet<string>>(
    () => new Set(latestId ? [latestId] : []),
  );
  const [previewId, setPreviewId] = useState<string>();
  const [previewOffset, setPreviewOffset] = useState(0);
  const [scrubId, setScrubId] = useState<string>();
  const list = useRef<HTMLDivElement>(null);
  const dragged = useRef(false);
  const tooltipId = useId();
  const itemKey = items.map((item) => item.id).join("\0");

  useLayoutEffect(() => {
    setCurrentIds((current) => {
      if (!latestId) return current.size ? new Set() : current;
      return items.some((item) => current.has(item.id)) ? current : new Set([latestId]);
    });
  }, [itemKey, latestId]);

  useLayoutEffect(() => {
    if (items.length < 4) return;
    const root = scrollElement.current;
    if (!root || typeof IntersectionObserver === "undefined") return;
    const observed = [...root.querySelectorAll<HTMLElement>("[data-user-message-id]")]
      .filter((element) => items.some((item) => item.id === element.dataset.userMessageId));
    const visible = new Set<string>();
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const id = (entry.target as HTMLElement).dataset.userMessageId;
        if (!id) continue;
        if (entry.isIntersecting) visible.add(id); else visible.delete(id);
      }
      if (visible.size) setCurrentIds(new Set(visible));
    }, { root, rootMargin: "-16px 0px 0px 0px" });
    for (const element of observed) observer.observe(element);
    return () => observer.disconnect();
  }, [itemKey, items, scrollElement]);

  useLayoutEffect(() => {
    if (scrubId) return;
    const current = items.find((item) => currentIds.has(item.id))?.id ?? latestId;
    list.current?.querySelector<HTMLElement>(`[data-user-message-rail-id="${safeSelector(current)}"]`)
      ?.scrollIntoView?.({ block: "nearest" });
  }, [currentIds, items, latestId, scrubId]);

  if (items.length < 4) return null;
  const preview = items.find((item) => item.id === previewId);
  const railButtonAt = (target: EventTarget | null) => target instanceof Element
      ? target.closest<HTMLButtonElement>("[data-user-message-rail-id]") : null;
  const itemAt = (target: EventTarget | null) => {
    const button = railButtonAt(target);
    const id = button?.dataset.userMessageRailId;
    return id && items.some((item) => item.id === id) ? id : undefined;
  };
  const endScrub = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (list.current?.hasPointerCapture?.(event.pointerId)) {
      list.current.releasePointerCapture?.(event.pointerId);
    }
    setScrubId(undefined);
  };
  const showPreview = (id: string, button: HTMLButtonElement) => {
    setPreviewId(id);
    setPreviewOffset(button.offsetTop + button.offsetHeight / 2 - (list.current?.scrollTop ?? 0));
  };

  return <nav className="thread-user-navigation" aria-label={t("timeline.userMessages")}
    onPointerLeave={() => { if (!scrubId) setPreviewId(undefined); }}>
    <div ref={list} className="thread-user-navigation-list"
      data-floating-navigation-rail-list="" data-scrubbing={scrubId ? "true" : undefined}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        const id = itemAt(event.target);
        if (!id) return;
        dragged.current = false; setScrubId(id);
        showPreview(id, railButtonAt(event.target)!);
        event.currentTarget.setPointerCapture?.(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!scrubId || event.buttons % 2 === 0) return;
        const element = document.elementFromPoint(event.clientX, event.clientY);
        const id = itemAt(element);
        if (!id || id === scrubId) return;
        dragged.current = true; setScrubId(id); showPreview(id, railButtonAt(element)!);
        onNavigate(id, "auto");
      }}
      onPointerUp={endScrub} onPointerCancel={endScrub} onLostPointerCapture={endScrub}>
      {items.map((item, index) => <button type="button" key={item.id}
        data-user-message-rail-id={item.id} data-scrub-target={scrubId === item.id ? "true" : undefined}
        aria-current={currentIds.has(item.id) ? "true" : undefined}
        aria-describedby={previewId === item.id ? tooltipId : undefined}
        aria-label={t("timeline.jumpUserMessage").replace("{position}", String(index + 1))}
        onPointerEnter={(event) => showPreview(item.id, event.currentTarget)}
        onFocus={(event) => showPreview(item.id, event.currentTarget)}
        onBlur={() => setPreviewId((current) => current === item.id ? undefined : current)}
        onClick={() => {
          if (dragged.current) { dragged.current = false; return; }
          setPreviewId(item.id); onNavigate(item.id, "smooth");
        }}><span className="thread-user-navigation-marker" aria-hidden="true">
          <span className="thread-user-navigation-marker-line" />
        </span></button>)}
    </div>
    {preview && <div className="thread-user-navigation-preview" id={tooltipId} role="tooltip"
      style={{ "--rail-preview-offset": `${previewOffset}px` } as CSSProperties}>
      <strong>{t("timeline.userMessagePreview").replace("{position}",
        String(items.findIndex((item) => item.id === preview.id) + 1))}</strong>
      <p>{preview.text}</p>
    </div>}
  </nav>;
}

function safeSelector(value: string | undefined): string {
  if (!value) return "";
  return typeof CSS !== "undefined" && CSS.escape ? CSS.escape(value) : value.replace(/"/g, "\\\"");
}
