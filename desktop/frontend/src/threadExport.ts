import type { WorkMessage } from "./state/workspace";

export function formatThreadMarkdown(
  title: string, messages: readonly WorkMessage[], labels: { readonly user: string; readonly assistant: string },
): string {
  const heading = title.replace(/\s+/g, " ").trim() || "Untitled work";
  const turns = messages.flatMap((message) => {
    const text = message.text.trim();
    if (!text) return [];
    const label = message.role === "user" ? labels.user : labels.assistant;
    return [`## ${label}\n\n${text}`];
  });
  return [`# ${heading}`, ...turns].join("\n\n") + "\n";
}
