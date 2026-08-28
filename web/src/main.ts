import "./style.css";
import { runFakeHost } from "./host";

const button = document.querySelector<HTMLButtonElement>("#run");
const output = document.querySelector<HTMLOutputElement>("#output");
if (!button || !output) throw new Error("missing application root controls");
button.addEventListener("click", () => { output.textContent = `${runFakeHost()} · completed`; });
