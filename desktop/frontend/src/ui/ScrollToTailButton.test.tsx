// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ScrollToTailButton } from "./ScrollToTailButton";

afterEach(cleanup);

describe("ScrollToTailButton", () => {
  it("keeps one bounded control and swaps unseen output into working dots", () => {
    const onClick = vi.fn();
    const view = render(<ScrollToTailButton visible working label="Jump to latest" onClick={onClick} />);
    const button = screen.getByRole("button", { name: "Jump to latest" });
    expect(button.getAttribute("data-visible")).toBe("true");
    expect(button.querySelectorAll(".conversation-tail-working span")).toHaveLength(3);
    expect(button.querySelector("svg")).toBeNull();
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();

    view.rerender(<ScrollToTailButton visible={false} working={false}
      label="Jump to latest" onClick={onClick} />);
    expect(button.getAttribute("aria-hidden")).toBe("true");
    expect(button.tabIndex).toBe(-1);
    expect(button.querySelector("svg")).not.toBeNull();
  });
});
