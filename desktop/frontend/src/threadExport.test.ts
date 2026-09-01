import { describe, expect, it } from "vitest";
import { formatThreadMarkdown } from "./threadExport";

describe("thread Markdown export", () => {
  it("exports admitted visible turns in order without technical session metadata", () => {
    expect(formatThreadMarkdown("  Release   review\n", [
      { id: "turn-1", role: "user", text: "Approve the release boundary" },
      { id: "turn-1-result", role: "assistant", text: "## Decision\n\nProceed.", terminal: "completed" },
      { id: "empty", role: "assistant", text: "", terminal: "stopped" },
    ], { user: "You", assistant: "Garive" })).toBe(
      "# Release review\n\n## You\n\nApprove the release boundary\n\n## Garive\n\n## Decision\n\nProceed.\n",
    );
  });
});
