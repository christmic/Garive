import { describe, expect, it } from "vitest";
import { TauriPreferenceBytesPort } from "./productStore";

describe("TauriPreferenceBytesPort", () => {
  it("keeps preferences and pending identity in separate typed commands", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const port = new TauriPreferenceBytesPort(async <T>(
      command: string, args: Record<string, unknown>,
    ) => {
      calls.push({ command, args });
      if (command === "read_client_preferences") return [123, 125] as T;
      if (command === "read_pending_command") return null as T;
      return undefined as T;
    });

    expect(await port.readPreferences()).toEqual(Uint8Array.from([123, 125]));
    expect(await port.readPendingCommand()).toBeUndefined();
    await port.writePreferences(Uint8Array.from([91, 93]));
    await port.writePendingCommand(undefined);

    expect(calls).toEqual([
      { command: "read_client_preferences", args: {} },
      { command: "read_pending_command", args: {} },
      { command: "write_client_preferences", args: { value: [91, 93] } },
      { command: "write_pending_command", args: { value: null } },
    ]);
  });

  it("rejects malformed, empty and oversized values before crossing boundaries", async () => {
    const malformed = new TauriPreferenceBytesPort(async <T>() => [256] as T);
    await expect(malformed.readPreferences()).rejects.toThrow("invalid_product_store_response");

    const port = new TauriPreferenceBytesPort(async <T>() => undefined as T);
    await expect(port.writePreferences(new Uint8Array())).rejects.toThrow("invalid_product_store_value");
    await expect(port.writePendingCommand(new Uint8Array(64 * 1024 + 1)))
      .rejects.toThrow("invalid_product_store_value");
  });
});
