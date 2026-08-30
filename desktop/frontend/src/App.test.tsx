/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App, type AppApi } from "./App";
import type { SetupCatalogue } from "./ipc/host";

const catalogue: SetupCatalogue = {
  schema_version: 1, catalogue_revision: "catalogue-1",
  profiles: [{
    profile_id: "profile-a", display_name_key: "setup.profile.a",
    endpoint_mode: "fixed", model_mode: "exact_id",
    credential_label_key: "setup.credential.connection", supported_capabilities: ["text"],
  }],
  presets: [{ preset_id: "preset-a", display_name_key: "setup.preset.balanced", supported_profile_ids: ["profile-a"] }],
  limits: { max_profiles: 1, max_text_bytes: 64, max_endpoint_bytes: 128, max_secret_bytes: 256, max_plan_count: 2, plan_lifetime_seconds: 60 },
};

afterEach(cleanup);

describe("Desktop configuration route", () => {
  it("offers redacted reconfiguration for an invalid stored document", async () => {
    render(<App api={api({ state: "invalid_configuration", code: "config_invalid_document" })} />);
    await screen.findByRole("heading", { name: "Garive needs reconfiguration" });
    expect(screen.getByRole("alert").textContent).not.toContain("config_invalid_document");
    fireEvent.click(screen.getByText("Diagnostics"));
    expect(screen.getByText("config_invalid_document")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reconfigure" }));
    await screen.findByRole("heading", { name: "Configure Garive" });
  });

  it("keeps reconfiguration explicit for a running immutable Runtime", async () => {
    render(<App api={api({ state: "configured", restart_required: false })} />);
    await screen.findByRole("heading", { name: "Garive is configured" });
    fireEvent.click(screen.getByRole("button", { name: "Reconfigure" }));
    await screen.findByRole("heading", { name: "Configure Garive" });
  });
});

function api(state: Awaited<ReturnType<AppApi["setupState"]>>): AppApi {
  return {
    setupState: async () => state,
    setupFlow: {
      catalogue: async () => catalogue,
      prepare: vi.fn(), commit: vi.fn(), cancel: vi.fn(), restart: vi.fn(),
    },
  };
}
