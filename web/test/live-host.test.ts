import { describe, expect, it } from "vitest";
import { FetchHostClient } from "../src/host";

const hostUrl = process.env.GARIVE_LIVE_HOST_URL;
const definitionId = process.env.GARIVE_LIVE_DEFINITION_ID;
const expected = process.env.GARIVE_LIVE_EXPECTED_TEXT;

describe.runIf(Boolean(hostUrl && definitionId && expected))("live Runtime Host", () => {
  it("completes a real durable turn through the Web transport", async () => {
    const client = new FetchHostClient(hostUrl!, {
      maxCommandBytes: 64 * 1024,
      maxEventBytes: 64 * 1024,
      maxEvents: 2_048,
      followDeadlineMs: 120_000,
    });
    const identity = `${process.pid}-${Date.now()}`;
    const session = await client.createSession(`web-create-${identity}`, definitionId!);
    const turn = await client.startTurn(
      `web-turn-${identity}`,
      session.session_id,
      `Reply with exactly ${expected} and no other characters.`,
    );

    const view = await client.followUntilTerminal(session.session_id, turn.committed_position);

    expect(view.terminal).toBe("completed");
    expect(view.text).toBe(expected);
  }, 120_000);
});
