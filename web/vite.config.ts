import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()], resolve: { dedupe: ["react", "react-dom"] },
  server: { host: "127.0.0.1", port: 1430, strictPort: true,
    proxy: { "/v1": "http://127.0.0.1:8787" } },
  build: { target: "es2023" }, test: { environment: "node" },
});
