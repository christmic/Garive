import { describe, expect, it } from "vitest";
import { runFakeHost } from "./host";

describe("desktop Host IPC", () => {
  it("returns one typed fake-host terminal", async () => {
    const calls: string[] = [];
    const result = await runFakeHost("hello", async <T>(command: string) => {
      calls.push(command);
      return "hello from Garive" as T;
    });
    expect(calls).toEqual(["run_fake_host"]);
    expect(result).toEqual({ text: "hello from Garive", terminal: "completed" });
  });
});
