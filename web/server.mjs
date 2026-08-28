import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
const root = new URL("./public/", import.meta.url).pathname;
const types = { ".html": "text/html; charset=utf-8", ".mjs": "text/javascript; charset=utf-8" };
const server = createServer(async (request, response) => {
  const path = request.url === "/" ? "index.html" : request.url.slice(1);
  if (path.includes("..")) { response.writeHead(400).end(); return; }
  try { response.setHeader("content-type", types[extname(path)] ?? "application/octet-stream"); response.end(await readFile(join(root, path))); }
  catch { response.writeHead(404).end("not found"); }
});
server.listen(Number(process.env.PORT ?? 4173), "127.0.0.1", () => console.log("Garive Web: http://127.0.0.1:4173"));
