import { describe, expect, it } from "vitest";
import { fakeEvents, runFakeHost } from "../src/host";

describe("Host API v1 fake session", () => {
  it("renders one ordered completion", () => expect(runFakeHost()).toBe("hello from Garive"));
  it("rejects EOF without a terminal", () =>
    expect(() => runFakeHost(fakeEvents.slice(0, -1))).toThrow(/terminal/));
});
