export const fakeEvents = [
  { position: 1, event: "session.created" }, { position: 2, event: "turn.started" },
  { position: 3, event: "output.delta", text: "hello " },
  { position: 4, event: "output.delta", text: "from Garive" }, { position: 5, event: "turn.completed" },
];
export function runFakeHost(events = fakeEvents) {
  let previous = 0, terminal = false, output = "";
  for (const item of events) { if (terminal || item.position !== ++previous) throw new Error("invalid Host API sequence");
    if (item.event === "output.delta") output += item.text; if (item.event === "turn.completed") terminal = true; }
  if (!terminal) throw new Error("missing Host API terminal"); return output;
}
if (typeof document !== "undefined") document.querySelector("#run")?.addEventListener("click", () => {
  document.querySelector("#output").textContent = `${runFakeHost()} · completed`;
});
