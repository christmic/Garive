import "./style.css";
import { FetchHostClient, HostClientError } from "./host";

const form = document.querySelector<HTMLFormElement>("#turn-form");
const baseUrl = document.querySelector<HTMLInputElement>("#base-url");
const definition = document.querySelector<HTMLInputElement>("#definition");
const input = document.querySelector<HTMLInputElement>("#input");
const output = document.querySelector<HTMLOutputElement>("#output");
if (!form || !baseUrl || !definition || !input || !output) throw new Error("missing application controls");
const controls = { baseUrl, definition, input, output };
form.addEventListener("submit", (event) => {
  event.preventDefault(); controls.output.textContent = "running";
  void run().catch((error: unknown) => {
    controls.output.textContent = error instanceof HostClientError ? error.code : "transport_failure";
  });
});
async function run(): Promise<void> {
  const client = new FetchHostClient(controls.baseUrl.value, {
    maxCommandBytes: 4_096, maxEventBytes: 8_192, maxEvents: 256, followDeadlineMs: 120_000,
  });
  const session = await client.createSession(`web-create-${crypto.randomUUID()}`, controls.definition.value);
  await client.startTurn(`web-turn-${crypto.randomUUID()}`, session.session_id, controls.input.value);
  const terminal = await client.followUntilTerminal(session.session_id);
  controls.output.textContent = terminal.terminal === "completed" ? terminal.text : terminal.terminal ?? "disconnected";
}
