import { describe, expect, it } from "vitest";
import { decodeDesktopMenuIntent } from "./desktopMenu";

describe("native Desktop menu intents", () => {
  it("accepts only data-free product navigation actions", () => {
    expect(decodeDesktopMenuIntent("desktop.new-work")).toBe("desktop.new-work");
    expect(decodeDesktopMenuIntent("desktop.search")).toBe("desktop.search");
    expect(decodeDesktopMenuIntent("desktop.settings")).toBe("desktop.settings");
    expect(decodeDesktopMenuIntent("desktop.toggle-inspector"))
      .toBe("desktop.toggle-inspector");
    expect(decodeDesktopMenuIntent("desktop.open-path")).toBeUndefined();
    expect(decodeDesktopMenuIntent({ path: "/Users/private" })).toBeUndefined();
  });
});
