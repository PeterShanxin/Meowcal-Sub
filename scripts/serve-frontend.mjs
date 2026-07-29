import { readFile, stat } from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const contentTypes = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".png", "image/png"],
  [".svg", "image/svg+xml"],
]);

function isInsideRoot(rootDirectory, candidatePath) {
  const root = path.resolve(rootDirectory).toLowerCase();
  const candidate = path.resolve(candidatePath).toLowerCase();
  return candidate === root || candidate.startsWith(`${root}${path.sep}`);
}

export function createStaticServer({ rootDirectory }) {
  const resolvedRoot = path.resolve(rootDirectory);

  return http.createServer(async (request, response) => {
    if (request.method !== "GET" && request.method !== "HEAD") {
      response.writeHead(405, { Allow: "GET, HEAD" });
      response.end();
      return;
    }

    try {
      const requestUrl = new URL(request.url ?? "/", "http://127.0.0.1");
      const decodedPath = decodeURIComponent(requestUrl.pathname);
      let filePath = path.resolve(resolvedRoot, `.${decodedPath}`);

      if (!isInsideRoot(resolvedRoot, filePath)) {
        response.writeHead(403);
        response.end();
        return;
      }

      const fileStat = await stat(filePath);
      if (fileStat.isDirectory()) {
        filePath = path.join(filePath, "index.html");
      }

      const body = await readFile(filePath);
      response.writeHead(200, {
        "Cache-Control": "no-store",
        "Content-Type": contentTypes.get(path.extname(filePath)) ?? "application/octet-stream",
        "X-Content-Type-Options": "nosniff",
      });
      response.end(request.method === "HEAD" ? undefined : body);
    } catch (error) {
      const status = error instanceof URIError ? 400 : 404;
      response.writeHead(status);
      response.end();
    }
  });
}

function readPort(argumentsList) {
  const portIndex = argumentsList.indexOf("--port");
  const rawPort = portIndex >= 0 ? argumentsList[portIndex + 1] : "3000";
  const port = Number.parseInt(rawPort, 10);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`Invalid port: ${rawPort}`);
  }
  return port;
}

const isCommandLine = process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url;
if (isCommandLine) {
  const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const server = createStaticServer({
    rootDirectory: path.join(repositoryRoot, "src"),
  });
  const port = readPort(process.argv.slice(2));

  server.listen(port, "127.0.0.1", () => {
    console.log(`Meowcal Sub frontend: http://127.0.0.1:${port}`);
  });
}
