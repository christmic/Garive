import type { ReactNode } from "react";

export function ComposerRail({ children, visible, placement = "above", variant = "default" }: {
  readonly children: ReactNode;
  readonly visible: boolean;
  readonly placement?: "above" | "below";
  readonly variant?: "default" | "warning" | "controls";
}) {
  const presence = visible ? "present" : "exiting";
  return <div className="composer-rail" data-composer-rail=""
    data-composer-rail-placement={placement}>
    <div className="composer-rail-item" data-composer-rail-item={presence}
      data-composer-rail-placement={placement} data-composer-rail-variant={variant}
      aria-hidden={visible ? undefined : true} inert={visible ? undefined : true}>
      <div className="composer-rail-content">{children}</div>
    </div>
  </div>;
}
