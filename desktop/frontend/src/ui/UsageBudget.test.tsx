// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { UsageBudgetCard, UsageBudgetTrigger, validUsageBudget,
  type UsageBudgetSnapshot } from "./UsageBudget";

const usage: UsageBudgetSnapshot = { source: "included_plan", state: "watch",
  scopeLabel: "Personal plan", periodLabel: "5-hour window", remainingPercent: 28,
  resetsAtLabel: "Resets in 1h 40m", attribution: "reported",
  modelPostureLabel: "Balanced", activeTurnMayFinish: true };
const copy = { title: "Usage & capacity", description: "Reported capacity for new work.",
  remaining: "remaining", reported: "Reported", estimated: "Estimated", reset: "Reset",
  modelPosture: "Cost posture", activeMayFinish: "Current work may finish.",
  activeMayStop: "Current work is subject to its execution budget." };

afterEach(cleanup);

describe("UsageBudget", () => {
  it("rejects invalid percentages instead of presenting plausible capacity", () => {
    expect(validUsageBudget(usage)).toBe(true);
    expect(validUsageBudget({ ...usage, remainingPercent: 101 })).toBe(false);
  });

  it("renders equivalent text for the meter and continuation policy", () => {
    render(<UsageBudgetCard value={usage} copy={copy} />);
    expect(screen.getByRole("progressbar", { name: "28% remaining" })).toBeTruthy();
    expect(screen.getByText("Current work may finish.")).toBeTruthy();
    expect(screen.getByText("Reported")).toBeTruthy();
  });

  it("keeps the compact trigger understandable without color", () => {
    render(<UsageBudgetTrigger value={usage} label="Capacity" onOpen={() => undefined} />);
    expect(screen.getByRole("button", { name: "Capacity: 28% · 5-hour window" })).toBeTruthy();
  });
});
