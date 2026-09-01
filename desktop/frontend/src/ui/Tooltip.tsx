import { cloneElement, useId, type ReactElement } from "react";

export function Tooltip({ label, shortcut, side = "bottom", align = "center", focusDisabled = false, children }: {
  label: string;
  shortcut?: string;
  side?: "top" | "bottom" | "right";
  align?: "start" | "center" | "end";
  focusDisabled?: boolean;
  children: ReactElement<{ "aria-describedby"?: string; "aria-label"?: string; "aria-hidden"?: boolean; disabled?: boolean }>;
}) {
  const id = useId();
  const description = [children.props["aria-describedby"], id].filter(Boolean).join(" ");
  const explainDisabled = focusDisabled && children.props.disabled;
  return <span className="ui-tooltip-anchor" data-side={side} data-align={align}
    {...(explainDisabled ? { role: "button", tabIndex: 0, "aria-disabled": true,
      "aria-label": children.props["aria-label"] ?? label, "aria-describedby": id } : {})}>
    {cloneElement(children, explainDisabled ? { "aria-hidden": true } : { "aria-describedby": description })}
    <span className="ui-tooltip" id={id} role="tooltip">
      <span>{label}</span>{shortcut && <kbd>{shortcut}</kbd>}
    </span>
  </span>;
}
