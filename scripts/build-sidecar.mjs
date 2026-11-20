// Builds the game server (dungerra-api) as the Tauri sidecar:
//   src-tauri/binaries/dungerra-server-<target-triple>[.exe]
//
// Usage: node scripts/build-sidecar.mjs [--target <rust-target-triple>]
// The target defaults to this machine's Rust host triple. The API checkout
// comes from DUNGERRA_API_DIR (default: ../dungerra-api next to this repo).
import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const apiDir = path.resolve(root, process.env.DUNGERRA_API_DIR ?? "../dungerra-api");
const outDir = path.join(root, "src-tauri", "binaries");

// Rust target triple → Go platform. The server is pure Go (SQLite included),
// so every target cross-compiles with CGO disabled.
const GO_PLATFORMS = {
  "x86_64-pc-windows-msvc": { goos: "windows", goarch: "amd64" },
  "aarch64-pc-windows-msvc": { goos: "windows", goarch: "arm64" },
  "x86_64-apple-darwin": { goos: "darwin", goarch: "amd64" },
  "aarch64-apple-darwin": { goos: "darwin", goarch: "arm64" },
  "x86_64-unknown-linux-gnu": { goos: "linux", goarch: "amd64" },
  "aarch64-unknown-linux-gnu": { goos: "linux", goarch: "arm64" },
};
const UNIVERSAL_MAC = "universal-apple-darwin";

function run(cmd, args, options = {}) {
  console.log(`> ${cmd} ${args.join(" ")}`);
  return execFileSync(cmd, args, { stdio: "inherit", ...options });
}

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const match = out.match(/^host: (\S+)$/m);
  if (!match) {
    throw new Error("Cannot read the host triple from `rustc -vV`");
  }
  return match[1];
}

function outputPath(triple) {
  const exe = triple.includes("windows") ? ".exe" : "";
  return path.join(outDir, `dungerra-server-${triple}${exe}`);
}

function goBuild(triple) {
  const platform = GO_PLATFORMS[triple];
  if (!platform) {
    throw new Error(`Unsupported target ${triple}. Known: ${[...Object.keys(GO_PLATFORMS), UNIVERSAL_MAC].join(", ")}`);
  }
  const out = outputPath(triple);
  run("go", ["build", "-trimpath", "-ldflags", "-s -w", "-o", out, "."], {
    cwd: apiDir,
    env: { ...process.env, CGO_ENABLED: "0", GOOS: platform.goos, GOARCH: platform.goarch },
  });
  return out;
}

const { values } = parseArgs({ options: { target: { type: "string" } } });
const target = values.target ?? hostTriple();
mkdirSync(outDir, { recursive: true });
console.log(`Building the game server from ${apiDir} for ${target}`);

if (target === UNIVERSAL_MAC) {
  // Tauri bundles one fat binary under the universal name; the per-arch
  // binaries stay too, since each per-arch cargo build checks for its own
  const parts = [goBuild("aarch64-apple-darwin"), goBuild("x86_64-apple-darwin")];
  run("lipo", ["-create", "-output", outputPath(UNIVERSAL_MAC), ...parts]);
} else {
  goBuild(target);
}
console.log(`Sidecar ready: ${outputPath(target)}`);
