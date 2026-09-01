// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ComposerRail } from "./ComposerRail";
import { Icon } from "./Icon";

describe("ComposerRail", () => {
  it("keeps one attached item so CSS can animate presence without losing semantics", () => {
    const view = render(<ComposerRail visible={false}><button>Inspect</button></ComposerRail>);
    const rail = view.container.querySelector("[data-composer-rail]");
    const item = view.container.querySelector<HTMLElement>("[data-composer-rail-item]");
    expect(rail?.getAttribute("data-composer-rail-placement")).toBe("above");
    expect(item?.getAttribute("data-composer-rail-item")).toBe("exiting");
    expect(item?.getAttribute("data-composer-rail-variant")).toBe("default");
    expect(item?.getAttribute("aria-hidden")).toBe("true");
    expect(item?.hasAttribute("inert")).toBe(true);

    view.rerender(<ComposerRail visible><button>Inspect</button></ComposerRail>);
    expect(item?.getAttribute("data-composer-rail-item")).toBe("present");
    expect(item?.hasAttribute("aria-hidden")).toBe(false);
    expect(item?.hasAttribute("inert")).toBe(false);
  });

  it("uses the source target glyph instead of a fabricated activity pulse", () => {
    const view = render(<Icon name="target" />);
    const target = view.container.querySelector("svg");
    expect(target?.getAttribute("viewBox")).toBe("0 0 20 20");
    expect(target?.querySelectorAll("path")).toHaveLength(3);
  });
});
