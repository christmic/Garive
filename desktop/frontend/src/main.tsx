import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./visual-system.css";
import "./style.css";

const root = document.querySelector<HTMLDivElement>("#root");
if (!root) throw new Error("missing application root");
document.documentElement.dataset.client = "desktop";
createRoot(root).render(<StrictMode><App /></StrictMode>);
