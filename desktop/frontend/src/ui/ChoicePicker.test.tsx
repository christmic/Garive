/** @vitest-environment jsdom */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChoicePicker } from "./ChoicePicker";

describe("ChoicePicker", () => {
  it("exposes product-owned pressed choices with roving keyboard focus", async () => {
    const change = vi.fn<(value: string) => void>();
    render(<ChoicePicker label="Runtime preset" value="balanced"
      options={[["balanced", "Balanced"], ["deep", "Deep"]]} onChange={change} />);

    const balanced = screen.getByRole("button", { name: "Balanced" });
    const deep = screen.getByRole("button", { name: "Deep" });
    expect(balanced.getAttribute("aria-pressed")).toBe("true");
    expect(balanced.tabIndex).toBe(0);
    expect(deep.tabIndex).toBe(-1);
    fireEvent.keyDown(balanced, { key: "ArrowRight" });
    expect(change).toHaveBeenLastCalledWith("deep");
    await waitFor(() => expect(document.activeElement).toBe(deep));
    fireEvent.keyDown(deep, { key: "Home" });
    expect(change).toHaveBeenLastCalledWith("balanced");
    await waitFor(() => expect(document.activeElement).toBe(balanced));
    fireEvent.click(balanced);
    expect(change).toHaveBeenLastCalledWith("balanced");
  });
});
