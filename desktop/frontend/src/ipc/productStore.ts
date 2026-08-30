import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { PreferenceBytesPort } from "../state/preferences";
import type { Invoke } from "./host";

const MAX_STORED_BYTES = 64 * 1024;

/** Narrow Tauri adapter for disposable preferences and exact pending identity. */
export class TauriPreferenceBytesPort implements PreferenceBytesPort {
  public constructor(private readonly invoke: Invoke = tauriInvoke) {}

  public async readPreferences(): Promise<Uint8Array | undefined> {
    return decodeBytes(await this.invoke<unknown>("read_client_preferences", {}));
  }

  public async writePreferences(value: Uint8Array): Promise<void> {
    await this.invoke<void>("write_client_preferences", { value: encodeBytes(value) });
  }

  public async readPendingCommand(): Promise<Uint8Array | undefined> {
    return decodeBytes(await this.invoke<unknown>("read_pending_command", {}));
  }

  public async writePendingCommand(value: Uint8Array | undefined): Promise<void> {
    await this.invoke<void>("write_pending_command", {
      value: value === undefined ? null : encodeBytes(value),
    });
  }
}

function decodeBytes(value: unknown): Uint8Array | undefined {
  if (value === null || value === undefined) return undefined;
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_STORED_BYTES ||
      value.some((byte) => !Number.isInteger(byte) || byte < 0 || byte > 255)) {
    throw new Error("invalid_product_store_response");
  }
  return Uint8Array.from(value as number[]);
}

function encodeBytes(value: Uint8Array): number[] {
  if (value.byteLength === 0 || value.byteLength > MAX_STORED_BYTES) {
    throw new Error("invalid_product_store_value");
  }
  return Array.from(value);
}
