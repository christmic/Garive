// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { ThreadSummarySection } from "./ThreadSummarySection";

describe("ThreadSummarySection", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    Object.defineProperty(window, "localStorage", { configurable: true, value: {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => values.set(key, value),
      clear: () => values.clear(),
    } });
  });

  it("progressively discloses content and remembers the user's choice", () => {
    const first = render(<ThreadSummarySection sectionKey="activity" title="Activity" count={3}>
      <span>Write scoped file</span>
    </ThreadSummarySection>);
    const toggle = screen.getByRole("button", { name: "Activity, 3" });
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(document.querySelector("#thread-summary-activity")?.hasAttribute("hidden")).toBe(true);
    first.unmount();
    render(<ThreadSummarySection sectionKey="activity" title="Activity" count={3}>
      <span>Write scoped file</span>
    </ThreadSummarySection>);
    expect(screen.getByRole("button", { name: "Activity, 3" }).getAttribute("aria-expanded")).toBe("false");
  });
});
