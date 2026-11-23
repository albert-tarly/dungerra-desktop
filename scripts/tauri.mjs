// Runs the Tauri CLI with the app version taken from dungerra-api's
// package.json, so the server, the web client and the desktop app share one
// version. The version fields in this repo (package.json, tauri.conf.json,
// Cargo.toml) are placeholders.
//
// Usage: node scripts/tauri.mjs <tauri args…>   e.g. build --target …
// The API checkout comes from DUNGERRA_API_DIR (default: ../dungerra-api).
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const apiDir = path.resolve(root, process.env.DUNGERRA_API_DIR ?? "../dungerra-api");

function apiVersion() {
  const pkg = JSON.parse(readFileSync(path.join(apiDir, "package.json"), "utf8"));
  if (!/^\d+\.\d+\.\d+$/.test(pkg.version ?? "")) {
    throw new Error(`dungerra-api package.json has no x.y.z version: ${pkg.version}`);
  }
  return pkg.version;
}

const version = apiVersion();
const [command, ...rest] = process.argv.slice(2);

// Only dev and build bundle the app; other commands (icon, info…) pass through
const args = [command, ...rest];
if (command === "dev" || command === "build") {
  const overrideDir = path.join(root, "src-tauri", "target");
  mkdirSync(overrideDir, { recursive: true });
  const override = path.join(overrideDir, "version.conf.json");
  writeFileSync(override, JSON.stringify({ version }));
  args.push("--config", override);
  console.log(`Dungerra ${version} (from ${path.relative(root, apiDir) || apiDir})`);
}

const cli = path.join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
const result = spawnSync(process.execPath, [cli, ...args], { stdio: "inherit" });
process.exit(result.status ?? 1);
