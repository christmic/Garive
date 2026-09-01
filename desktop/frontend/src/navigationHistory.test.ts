import { describe, expect, it } from "vitest";
import { canNavigate, createNavigationHistory, moveNavigation, pushNavigation } from "./navigationHistory";

describe("application navigation history", () => {
  it("branches after Back and de-duplicates the current destination", () => {
    let history = createNavigationHistory();
    history = pushNavigation(history, { kind: "agents" });
    history = pushNavigation(history, { kind: "settings", section: "general" });
    history = moveNavigation(history, -1);
    history = pushNavigation(history, { kind: "session", sessionId: "session-1" });
    expect(history.entries.map((entry) => entry.kind)).toEqual(["new-work", "agents", "session"]);
    expect(pushNavigation(history, { kind: "session", sessionId: "session-1" })).toBe(history);
    expect(canNavigate(history, 1)).toBe(false);
  });

  it("keeps a bounded fifty-entry working set", () => {
    let history = createNavigationHistory();
    for (let index = 0; index < 70; index += 1) {
      history = pushNavigation(history, { kind: "session", sessionId: `session-${index}` });
    }
    expect(history.entries).toHaveLength(50);
    expect(history.entries[0]).toEqual({ kind: "session", sessionId: "session-20" });
    expect(canNavigate(history, -1)).toBe(true);
  });
});
