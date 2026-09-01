export type SettingsDestination = "general" | "usage" | "workspace" | "runtime" |
  "updates" | "privacy";

export type AppDestination =
  | { readonly kind: "new-work" }
  | { readonly kind: "session"; readonly sessionId: string }
  | { readonly kind: "agents" }
  | { readonly kind: "settings"; readonly section: SettingsDestination };

export interface AppNavigationHistory {
  readonly entries: readonly AppDestination[];
  readonly index: number;
}

const MAX_ENTRIES = 50;

export function createNavigationHistory(
  initial: AppDestination = { kind: "new-work" },
): AppNavigationHistory {
  return { entries: [initial], index: 0 };
}

export function destinationKey(destination: AppDestination): string {
  if (destination.kind === "session") return `session:${destination.sessionId}`;
  if (destination.kind === "settings") return `settings:${destination.section}`;
  return destination.kind;
}

export function pushNavigation(history: AppNavigationHistory,
  destination: AppDestination): AppNavigationHistory {
  if (destinationKey(history.entries[history.index]!) === destinationKey(destination)) return history;
  const branched = [...history.entries.slice(0, history.index + 1), destination];
  const entries = branched.slice(-MAX_ENTRIES);
  return { entries, index: entries.length - 1 };
}

export function moveNavigation(history: AppNavigationHistory,
  direction: -1 | 1): AppNavigationHistory {
  const index = Math.max(0, Math.min(history.entries.length - 1, history.index + direction));
  return index === history.index ? history : { ...history, index };
}

export function canNavigate(history: AppNavigationHistory, direction: -1 | 1): boolean {
  return direction < 0 ? history.index > 0 : history.index < history.entries.length - 1;
}
