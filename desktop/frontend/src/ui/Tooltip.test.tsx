// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Tooltip } from "./Tooltip";

describe("Tooltip", () => {
  it("binds a styled tooltip and shortcut to its control", () => {
    render(<Tooltip label="Toggle inspector" shortcut="⌘⇧A" side="top" align="end">
      <button type="button" aria-label="Toggle inspector" aria-describedby="existing">Panel</button>
    </Tooltip>);

    const button = screen.getByRole("button", { name: "Toggle inspector" });
    const tooltip = screen.getByRole("tooltip");
    expect(button.getAttribute("aria-describedby")).toBe(`existing ${tooltip.id}`);
    expect(tooltip.textContent).toBe("Toggle inspector⌘⇧A");
    expect(tooltip.parentElement?.dataset).toMatchObject({ side: "top", align: "end" });
    expect(button.hasAttribute("title")).toBe(false);
  });

  it("optionally exposes a disabled explanation to the keyboard", () => {
    render(<Tooltip label="No Workspace available" focusDisabled>
      <button type="button" aria-label="Add context" disabled>+</button>
    </Tooltip>);

    const trigger = screen.getByRole("button", { name: "Add context" });
    expect(trigger.tagName).toBe("SPAN");
    expect(trigger.getAttribute("aria-disabled")).toBe("true");
    expect(trigger.tabIndex).toBe(0);
    expect(trigger.querySelector("button")?.getAttribute("aria-hidden")).toBe("true");
  });
});
