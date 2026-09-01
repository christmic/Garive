// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { clearThreadFindMatches, findThreadTextMatches } from "./threadFind";

describe("thread find source contract", () => {
  it("finds case-insensitively across inline nodes without crossing Turn units", () => {
    const root = document.createElement("div");
    root.innerHTML = `<article data-thread-find-unit>Durable <strong>Runtime</strong> truth</article>
      <article data-thread-find-unit>runtime recovery</article>`;
    const result = findThreadTextMatches(root, "durable runtime");
    expect(result.matches).toHaveLength(1);
    expect(result.matches[0]?.textContent).toBe("Durable Runtime");
    expect(root.querySelectorAll("mark[data-search-match]")).toHaveLength(1);
    clearThreadFindMatches(root);
    expect(root.textContent).toContain("Durable Runtime truth");
    expect(root.querySelector("mark")).toBeNull();
  });

  it("skips governed controls and reports a bounded result set", () => {
    const root = document.createElement("div");
    root.innerHTML = `<article data-thread-find-unit>work work work
      <span data-thread-find-skip>work</span></article>`;
    const result = findThreadTextMatches(root, "work", 2);
    expect(result.matches).toHaveLength(2);
    expect(result.capped).toBe(true);
  });
});
