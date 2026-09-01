import { describe, expect, it } from "vitest";
import { classifyTask, filterAndOrderTasks, groupSidebarTasks, summarizeTasks,
  type RecentTask } from "./taskPresentation";

const tasks: readonly RecentTask[] = [
  { session_id: "completed", definition_id: "agent", opened_at: "2026-08-30T10:00:00Z",
    latest_turn_state: "completed", turn_count: 2 },
  { session_id: "running", definition_id: "agent", opened_at: "2026-08-30T11:00:00Z",
    latest_turn_state: "running", turn_count: 1 },
  { session_id: "attention", definition_id: "agent", opened_at: "2026-08-30T09:00:00Z",
    latest_turn_state: "suspended", turn_count: 3 },
  { session_id: "failed", definition_id: "agent", opened_at: "2026-08-30T12:00:00Z",
    latest_turn_state: "failed", turn_count: 1 },
];

describe("durable task presentation", () => {
  it("classifies lifecycle states without inventing progress", () => {
    expect(tasks.map(classifyTask)).toEqual(["completed", "active", "attention", "failed"]);
    expect(classifyTask({ session_id: "empty" })).toBe("idle");
  });

  it("orders attention before active, failure and completed work", () => {
    expect(filterAndOrderTasks(tasks, "all", "", {}).map((item) => item.session_id))
      .toEqual(["attention", "running", "failed", "completed"]);
  });

  it("filters by actionable state and human title", () => {
    const titles = { attention: "Approve launch memo", completed: "Quarterly plan" };
    expect(filterAndOrderTasks(tasks, "attention", "approve", titles).map((item) => item.session_id))
      .toEqual(["attention"]);
    expect(filterAndOrderTasks(tasks, "completed", "quarterly", titles).map((item) => item.session_id))
      .toEqual(["completed"]);
  });

  it("summarizes work requiring intervention separately from active work", () => {
    expect(summarizeTasks(tasks)).toEqual({ attention: 1, active: 1, failed: 1, completed: 1, total: 4 });
  });

  it("groups truthful priority work ahead of quiet recents without duplication", () => {
    expect(groupSidebarTasks(filterAndOrderTasks(tasks, "all", "", {}))).toEqual([
      { kind: "priority", tasks: [tasks[2], tasks[1], tasks[3]] },
      { kind: "recent", tasks: [tasks[0]] },
    ]);
    const limited = groupSidebarTasks(tasks, 2).flatMap((group) => group.tasks);
    expect(limited).toHaveLength(2);
    expect(new Set(limited.map((task) => task.session_id))).toEqual(new Set(["completed", "running"]));
  });
});
