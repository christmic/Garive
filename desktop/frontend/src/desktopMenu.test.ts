import { describe, expect, it } from "vitest";
import { decodeDesktopMenuIntent } from "./desktopMenu";

describe("native Desktop menu intents", () => {
  it("accepts only data-free product navigation actions", () => {
    expect(decodeDesktopMenuIntent("desktop.new-work")).toBe("desktop.new-work");
    expect(decodeDesktopMenuIntent("desktop.search")).toBe("desktop.search");
    expect(decodeDesktopMenuIntent("desktop.settings")).toBe("desktop.settings");
    expect(decodeDesktopMenuIntent("desktop.toggle-inspector"))
      .toBe("desktop.toggle-inspector");
    expect(decodeDesktopMenuIntent("desktop.zoom-in")).toBe("desktop.zoom-in");
    expect(decodeDesktopMenuIntent("desktop.zoom-out")).toBe("desktop.zoom-out");
    expect(decodeDesktopMenuIntent("desktop.actual-size")).toBe("desktop.actual-size");
    expect(decodeDesktopMenuIntent("desktop.open-path")).toBeUndefined();
    expect(decodeDesktopMenuIntent({ path: "/Users/private" })).toBeUndefined();
  });
});
