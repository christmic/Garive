// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createTranslator } from "../i18n";
import { ThreadFindBar } from "./ThreadFindBar";

afterEach(() => vi.useRealTimers());

describe("ThreadFindBar", () => {
  it("debounces cross-node search and cycles results in both directions", () => {
    vi.useFakeTimers();
    const root = document.createElement("div");
    root.innerHTML = `<article data-thread-find-unit>Durable <strong>Runtime</strong></article>
      <article data-thread-find-unit>Runtime recovery</article>`;
    const onClose = vi.fn();
    const view = render(<ThreadFindBar open openRevision={1} container={{ current: root }}
      onClose={onClose} t={createTranslator("en")} />);
    const input = screen.getByRole("textbox", { name: "Find in chat" });
    fireEvent.change(input, { target: { value: "runtime" } });
    act(() => vi.advanceTimersByTime(149));
    expect(root.querySelector("mark")).toBeNull();
    act(() => vi.advanceTimersByTime(1));
    expect(root.querySelectorAll("mark[data-search-match]")).toHaveLength(2);
    expect(root.querySelectorAll("mark[data-active]")).toHaveLength(1);
    expect(screen.getByText("1 / 2 results")).toBeTruthy();

    fireEvent.keyDown(input, { key: "Enter" });
    expect(root.querySelectorAll("mark")[1]?.hasAttribute("data-active")).toBe(true);
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    expect(root.querySelectorAll("mark")[0]?.hasAttribute("data-active")).toBe(true);
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
    expect(root.querySelector("mark")).toBeNull();
    view.unmount();
  });

  it("keeps the source result controls disabled for no matches", () => {
    vi.useFakeTimers();
    const root = document.createElement("div");
    root.innerHTML = `<article data-thread-find-unit>Committed result</article>`;
    render(<ThreadFindBar open openRevision={1} container={{ current: root }}
      onClose={vi.fn()} t={createTranslator("en")} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "missing" } });
    act(() => vi.advanceTimersByTime(150));
    expect(screen.getByText("0 results")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Previous result" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Next result" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
