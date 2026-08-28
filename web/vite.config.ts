import { defineConfig } from "vitest/config";

export default defineConfig({ build: { target: "es2023" }, test: { environment: "node" } });
