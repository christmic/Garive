import { useState } from "react";
import { runFakeHost } from "./ipc/host";

export function App() {
  const [output, setOutput] = useState("");
  const [error, setError] = useState("");
  const run = async () => {
    try { const result = await runFakeHost("hello"); setOutput(`${result.text} · ${result.terminal}`); setError(""); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "host invocation failed"); }
  };
  return <main><h1>Garive Desktop Agent</h1><p>You: hello</p>
    <button type="button" onClick={run}>Run embedded host</button>
    <output>{output}</output>{error && <p role="alert">{error}</p>}</main>;
}
