// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { createTranslator } from "../i18n";
import { ThreadUserNavigationRail } from "./ThreadUserNavigationRail";

const items = ["First request", "Refine the brief", "Check the evidence", "Ship the result"]
  .map((text, index) => ({ id: `user-${index + 1}`, text }));
const scrollElement = { current: document.createElement("div") };

describe("ThreadUserNavigationRail", () => {
  it("stays absent until a thread has four user messages", () => {
    const view = render(<ThreadUserNavigationRail items={[]}
      scrollElement={scrollElement} onNavigate={vi.fn()} t={createTranslator("en")} />);
    expect(view.container.querySelector("nav")).toBeNull();
    view.rerender(<ThreadUserNavigationRail items={items.slice(0, 3)}
      scrollElement={scrollElement} onNavigate={vi.fn()} t={createTranslator("en")} />);
    expect(view.container.querySelector("nav")).toBeNull();
  });

  it("exposes real message anchors and starts on the latest message", () => {
    const onNavigate = vi.fn();
    const view = render(<ThreadUserNavigationRail items={items} scrollElement={scrollElement}
      onNavigate={onNavigate} t={createTranslator("en")} />);
    const buttons = screen.getAllByRole("button");
    expect(screen.getByRole("navigation", { name: "User messages" })).toBeTruthy();
    expect(buttons).toHaveLength(4);
    expect(buttons[3]?.getAttribute("aria-current")).toBe("true");

    fireEvent.focus(buttons[1]!);
    expect(screen.getByRole("tooltip").textContent).toContain("Refine the brief");
    expect(buttons[1]?.getAttribute("aria-describedby")).toBeTruthy();
    fireEvent.click(buttons[1]!);
    expect(onNavigate).toHaveBeenCalledWith("user-2", "smooth");

    const nextItems = items.map((item, index) => ({ ...item, id: `next-${index + 1}` }));
    view.rerender(<ThreadUserNavigationRail items={nextItems} scrollElement={scrollElement}
      onNavigate={onNavigate} t={createTranslator("en")} />);
    expect(screen.getAllByRole("button")[3]?.getAttribute("aria-current")).toBe("true");
  });
});
