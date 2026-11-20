// Builds the web client's desktop target and copies it to
//   src-tauri/resources/web/   (bundled as the app resource "web/")
//
// Usage: node scripts/build-web.mjs [--install]
// --install runs `npm ci` in the client first (CI does this). The client
// checkout comes from DUNGERRA_DIR (default: ../dungerra next to this repo).
import { execFileSync, execSync } from "node:child_process";
import { cpSync, existsSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const clientDir = path.resolve(root, process.env.DUNGERRA_DIR ?? "../dungerra");
const builtDir = path.join(clientDir, "dist-desktop");
const outDir = path.join(root, "src-tauri", "resources", "web");

function npm(args) {
  console.log(`> npm ${args.join(" ")}`);
  const options = { cwd: clientDir, stdio: "inherit" };
  // Under `npm run`, npm_execpath is npm's own CLI script: running it with
  // node avoids npm.cmd on Windows, which needs a shell
  if (process.env.npm_execpath?.endsWith(".js")) {
    execFileSync(process.execPath, [process.env.npm_execpath, ...args], options);
  } else {
    execSync(`npm ${args.join(" ")}`, options); // args are fixed words, nothing to escape
  }
}

const { values } = parseArgs({ options: { install: { type: "boolean", default: false } } });
console.log(`Building the web client from ${clientDir}`);
if (values.install || !existsSync(path.join(clientDir, "node_modules"))) {
  npm(["ci"]);
}
npm(["run", "build:desktop"]);
if (!existsSync(path.join(builtDir, "index.html"))) {
  throw new Error(`${builtDir} has no index.html. Does this client version have the build:desktop script?`);
}

rmSync(outDir, { recursive: true, force: true });
cpSync(builtDir, outDir, { recursive: true });
console.log(`Web client ready: ${outDir}`);
