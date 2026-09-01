/** @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SetupCatalogue, SetupInput, SetupPlan } from "../../ipc/host";
import { createTranslator } from "../../i18n";
import { SetupFlow, type SetupFlowApi } from "./SetupFlow";

const catalogue: SetupCatalogue = {
  schema_version: 1,
  catalogue_revision: "catalogue-1",
  profiles: [{
    profile_id: "profile-a", display_name_key: "setup.profile.a",
    endpoint_mode: "optional_override", model_mode: "exact_id",
    credential_label_key: "setup.credential.connection", supported_capabilities: ["text"],
  }],
  presets: [{
    preset_id: "preset-a", display_name_key: "setup.preset.balanced",
    supported_profile_ids: ["profile-a"],
  }],
  limits: {
    max_profiles: 2, max_text_bytes: 256, max_endpoint_bytes: 2048,
    max_secret_bytes: 16384, max_plan_count: 16, plan_lifetime_seconds: 900,
  },
};

afterEach(cleanup);

describe("secure Desktop setup", () => {
  it("reviews redacted choices then clears the write-only credential", async () => {
    let prepared: SetupInput | undefined;
    let committedCredential = "";
    const api = setupApi(async (input) => {
      prepared = input;
      return plan(input);
    }, async (_digest, credential) => {
      committedCredential = credential;
    });
    render(<SetupFlow api={api} nonce={() => "nonce-1"} />);

    await screen.findByRole("heading", { name: "Configure Garive" });
    fireEvent.change(screen.getByLabelText("Model target"), { target: { value: "target-a" } });
    fireEvent.change(screen.getByLabelText("Model ID"), { target: { value: "model-a" } });
    expect(screen.queryByLabelText("Deployment")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await screen.findByLabelText("Deployment");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByLabelText("Deployment")));
    fireEvent.change(screen.getByLabelText("Deployment"), { target: { value: "deployment-a" } });
    fireEvent.change(screen.getByLabelText("Agent definition"), { target: { value: "definition-a" } });
    fireEvent.click(screen.getByRole("button", { name: "Review setup" }));

    await screen.findByRole("heading", { name: "Review setup" });
    expect(prepared?.preset_id).toBe("preset-a");
    const secret = screen.getByLabelText("Credential") as HTMLInputElement;
    expect(secret.type).toBe("password");
    fireEvent.change(secret, { target: { value: "secret-once" } });
    fireEvent.click(screen.getByRole("button", { name: "Commit configuration" }));

    await screen.findByRole("heading", { name: "Restart required" });
    expect(committedCredential).toBe("secret-once");
    expect(secret.value).toBe("");
    expect(document.body.textContent).not.toContain("secret-once");
  });

  it("clears the credential and returns focus when commit fails", async () => {
    const api = setupApi(async (input) => plan(input), async () => {
      throw new Error("setup_persistence_failed");
    });
    render(<SetupFlow api={api} nonce={() => "nonce-2"} />);
    await fillRequiredDetails();
    const secret = screen.getByLabelText("Credential") as HTMLInputElement;
    fireEvent.change(secret, { target: { value: "discard-me" } });
    fireEvent.click(screen.getByRole("button", { name: "Commit configuration" }));
    await screen.findByRole("alert");
    expect(secret.value).toBe("");
    await waitFor(() => expect(document.activeElement).toBe(secret));
  });

  it("completes the secure journey in Simplified Chinese", async () => {
    const api = setupApi(async (input) => plan(input), async () => undefined);
    render(<SetupFlow api={api} nonce={() => "nonce-zh"}
      t={createTranslator("zh-Hans")} />);
    await screen.findByRole("heading", { name: "配置 Garive" });
    for (const [label, value] of [
      ["模型目标", "target-a"], ["模型 ID", "model-a"],
    ]) fireEvent.change(screen.getByLabelText(label), { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "继续" }));
    await screen.findByLabelText("部署");
    for (const [label, value] of [
      ["部署", "deployment-a"], ["智能体定义", "definition-a"],
    ]) fireEvent.change(screen.getByLabelText(label), { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "检查设置" }));
    await screen.findByRole("heading", { name: "检查设置" });
    fireEvent.change(screen.getByLabelText("凭据"), { target: { value: "secret-once" } });
    fireEvent.click(screen.getByRole("button", { name: "提交配置" }));
    await screen.findByRole("heading", { name: "需要重新启动" });
  });

  it("preserves each progressive group when navigating back", async () => {
    render(<SetupFlow api={setupApi(async (input) => plan(input), async () => undefined)} />);
    await screen.findByRole("heading", { name: "Configure Garive" });
    fireEvent.change(screen.getByLabelText("Model target"), { target: { value: "target-a" } });
    fireEvent.change(screen.getByLabelText("Model ID"), { target: { value: "model-a" } });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const deployment = await screen.findByLabelText("Deployment") as HTMLInputElement;
    fireEvent.change(deployment, { target: { value: "deployment-a" } });
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.queryByLabelText("Deployment")).toBeNull();
    expect((screen.getByLabelText("Model target") as HTMLInputElement).value).toBe("target-a");
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect((await screen.findByLabelText("Deployment") as HTMLInputElement).value).toBe("deployment-a");
  });
});

async function fillRequiredDetails() {
  await screen.findByRole("heading", { name: "Configure Garive" });
  for (const [label, value] of [
    ["Model target", "target-a"], ["Model ID", "model-a"],
  ]) fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  await screen.findByLabelText("Deployment");
  for (const [label, value] of [
    ["Deployment", "deployment-a"], ["Agent definition", "definition-a"],
  ]) fireEvent.change(screen.getByLabelText(label), { target: { value } });
  fireEvent.click(screen.getByRole("button", { name: "Review setup" }));
  await screen.findByRole("heading", { name: "Review setup" });
}

function plan(input: SetupInput): SetupPlan {
  return {
    schema_version: 1, setup_id: "setup-1", caller_nonce: input.caller_nonce,
    catalogue_revision: input.catalogue_revision, effective_configuration_digest: "a".repeat(64),
    expires_at: "2030-01-01T00:00:00Z", summary: { ...input, endpoint_mode: "fixed" },
    plan_digest: "b".repeat(64),
  };
}

function setupApi(
  prepare: (input: SetupInput) => Promise<SetupPlan>,
  commit: (digest: string, credential: string) => Promise<void>,
): SetupFlowApi {
  return {
    catalogue: async () => catalogue, prepare, commit,
    cancel: vi.fn(async () => "cancelled"), restart: vi.fn(async () => undefined),
  };
}
