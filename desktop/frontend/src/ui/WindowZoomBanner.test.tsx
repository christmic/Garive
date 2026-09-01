// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createTranslator } from "../i18n";
import { WindowZoomBanner } from "./WindowZoomBanner";

afterEach(() => vi.useRealTimers());

describe("WindowZoomBanner", () => {
  it("shows exact zoom, actions, and dismisses after the source-backed dwell", () => {
    vi.useFakeTimers(); const onStep = vi.fn(); const onReset = vi.fn();
    const view = render(<WindowZoomBanner zoom={1.2} revision={1} onStep={onStep}
      onReset={onReset} t={createTranslator("en")} />);
    expect(screen.getByText("120%")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onStep).toHaveBeenCalledWith(-1); expect(onReset).toHaveBeenCalledOnce();
    act(() => vi.advanceTimersByTime(2_000));
    expect(view.container.querySelector(".window-zoom-banner")).toBeNull();
  });

  it("pauses dismissal while the user continues adjusting zoom", () => {
    vi.useFakeTimers();
    const view = render(<WindowZoomBanner zoom={0.8} revision={1} onStep={vi.fn()}
      onReset={vi.fn()} t={createTranslator("en")} />);
    const banner = screen.getByTestId("window-zoom-banner");
    fireEvent.mouseEnter(banner); act(() => vi.advanceTimersByTime(2_500));
    expect(screen.getByText("80%")).toBeTruthy();
    fireEvent.mouseLeave(banner); act(() => vi.runOnlyPendingTimers());
    expect(view.container.querySelector(".window-zoom-banner")).toBeNull();
  });
});
