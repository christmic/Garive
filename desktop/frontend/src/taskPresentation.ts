export interface RecentTask {
  readonly session_id: string; readonly definition_id?: string; readonly opened_at?: string;
  readonly latest_turn_state?: "running" | "completed" | "suspended" | "stopped" | "failed";
  readonly turn_count?: number;
}

export type TaskCategory = "attention" | "active" | "failed" | "completed" | "idle";
export type TaskFilter = "all" | "attention" | "active" | "completed";

export function classifyTask(task: RecentTask): TaskCategory {
  if (task.latest_turn_state === "suspended") return "attention";
  if (task.latest_turn_state === "running") return "active";
  if (task.latest_turn_state === "failed") return "failed";
  if (task.latest_turn_state === "completed" || task.latest_turn_state === "stopped") return "completed";
  return "idle";
}

export function summarizeTasks(tasks: readonly RecentTask[]) {
  const summary = { attention: 0, active: 0, failed: 0, completed: 0, total: tasks.length };
  for (const task of tasks) {
    const category = classifyTask(task);
    if (category !== "idle") summary[category] += 1;
  }
  return summary;
}

export function filterAndOrderTasks(
  tasks: readonly RecentTask[], filter: TaskFilter, query: string,
  titles: Readonly<Record<string, string>>,
): readonly RecentTask[] {
  const needle = query.trim().toLocaleLowerCase();
  return [...tasks].filter((task) => {
    const category = classifyTask(task);
    const matchesFilter = filter === "all" || filter === "completed"
      ? filter === "all" || category === "completed"
      : category === filter;
    const searchable = `${titles[task.session_id] ?? ""} ${task.definition_id ?? ""}`.toLocaleLowerCase();
    return matchesFilter && (!needle || searchable.includes(needle));
  }).sort((left, right) => {
    const priority: Record<TaskCategory, number> = { attention: 0, active: 1, failed: 2, idle: 3, completed: 4 };
    const categoryOrder = priority[classifyTask(left)] - priority[classifyTask(right)];
    if (categoryOrder) return categoryOrder;
    return Date.parse(right.opened_at ?? "") - Date.parse(left.opened_at ?? "");
  });
}
