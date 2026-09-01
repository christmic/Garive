import { useEffect, useRef, useState } from "react";
import type { MessageKey } from "../i18n";
import { Icon } from "./Icon";

export function WindowZoomBanner({ zoom, revision, onStep, onReset, t }: {
  readonly zoom: number; readonly revision: number;
  readonly onStep: (direction: -1 | 1) => void; readonly onReset: () => void;
  readonly t: (key: MessageKey) => string;
}) {
  const [visible, setVisible] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  const deadline = useRef(0);
  const hovering = useRef(false);
  const schedule = (delay: number) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setVisible(false), delay);
  };
  useEffect(() => {
    if (!revision) return;
    setVisible(true); deadline.current = Date.now() + 2_000;
    if (!hovering.current) schedule(2_000);
    return () => window.clearTimeout(timer.current);
  }, [revision]);
  if (!visible) return null;
  const percent = Math.round(zoom * 100);
  return <div className="window-zoom-banner" data-testid="window-zoom-banner"
    onMouseEnter={() => { hovering.current = true; window.clearTimeout(timer.current); }}
    onMouseLeave={() => { hovering.current = false;
      schedule(Math.max(0, deadline.current - Date.now())); }}>
    <span className="window-zoom-percent" aria-live="polite">
      {t("shell.zoomPercent").replace("{percent}", String(percent))}</span>
    <div className="window-zoom-steps">
      <button type="button" aria-label={t("shell.zoomOut")} onClick={() => onStep(-1)}>
        <Icon name="minus" /></button>
      <button type="button" aria-label={t("shell.zoomIn")} onClick={() => onStep(1)}>
        <Icon name="plus" /></button>
    </div><span className="window-zoom-divider" aria-hidden="true" />
    <button className="window-zoom-reset" type="button" disabled={zoom === 1}
      onClick={onReset}>{t("shell.zoomReset")}</button>
  </div>;
}
