// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TurnActionControls } from "./TurnActionControls";

describe("TurnActionControls", () => {
  it("keeps completed status quiet while preserving the source action order", () => {
    const view = render(<TurnActionControls terminal="completed" terminalLabel="Completed">
      <button type="button">Copy</button><button type="button">Export</button>
    </TurnActionControls>);
    expect(view.container.querySelector(".result-terminal")?.classList.contains("sr-only")).toBe(true);
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Copy", "Export"]);
  });

  it("keeps non-success terminal evidence visible without changing row geometry", () => {
    const view = render(<TurnActionControls terminal="failed" terminalLabel="Failed">
      <button type="button">Copy</button>
    </TurnActionControls>);
    expect(view.container.querySelector("[data-turn-action-controls]")?.getAttribute("data-terminal"))
      .toBe("failed");
    expect(view.container.querySelector(".result-terminal")?.classList.contains("sr-only")).toBe(false);
  });
});
