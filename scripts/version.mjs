#!/usr/bin/env node
// Single place to change the app version.
//
//   node scripts/version.mjs 0.2.0        set an explicit version
//   node scripts/version.mjs patch        bump patch | minor | major
//   node scripts/version.mjs --check      verify that everything is in sync
//   node scripts/version.mjs --print      print the current version
//
// Source of truth: [workspace.package] version in Cargo.toml (all crates
// inherit it). Synchronised copies: app/package.json, app/package-lock.json,
// app/src-tauri/tauri.conf.json and the workspace entries in Cargo.lock.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const p = (...parts) => join(root, ...parts);
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

const read = (file) => readFileSync(file, "utf8");
const write = (file, text) => writeFileSync(file, text);

// Replaces the first `count` "version" values, leaving formatting untouched.
function jsonVersion(file, next, count = 1) {
  let n = 0;
  write(
    file,
    read(file).replace(/("version"\s*:\s*")[^"]*(")/g, (m, a, b) => (n++ < count ? a + next + b : m)),
  );
}

const workspaceRe = /(\[workspace\.package\][^[]*?\nversion\s*=\s*")([^"]+)(")/;

function readVersions() {
  const cargoToml = read(p("Cargo.toml"));
  const m = workspaceRe.exec(cargoToml);
  if (!m) throw new Error("[workspace.package] version not found in Cargo.toml");
  const lock = JSON.parse(read(p("app/package-lock.json")));
  const versions = {
    "Cargo.toml": m[2],
    "app/package.json": JSON.parse(read(p("app/package.json"))).version,
    "app/package-lock.json": lock.version,
    "app/package-lock.json (packages[''])": lock.packages?.[""]?.version,
    "app/src-tauri/tauri.conf.json": JSON.parse(read(p("app/src-tauri/tauri.conf.json"))).version,
  };
  // Workspace crates in Cargo.lock are the packages without a registry source.
  for (const block of read(p("Cargo.lock")).split(/\r?\n\r?\n/)) {
    if (!block.includes("[[package]]") || /^source\s*=/m.test(block)) continue;
    const name = /^name\s*=\s*"([^"]+)"/m.exec(block)?.[1];
    const version = /^version\s*=\s*"([^"]+)"/m.exec(block)?.[1];
    if (name) versions[`Cargo.lock (${name})`] = version;
  }
  return versions;
}

function bump(current, kind) {
  const [major, minor, patch] = current.split("-")[0].split(".").map(Number);
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

const arg = process.argv[2];
if (!arg) {
  console.error("Usage: node scripts/version.mjs <x.y.z | major | minor | patch | --check | --print>");
  process.exit(2);
}

const versions = readVersions();
const current = versions["Cargo.toml"];

if (arg === "--print") {
  console.log(current);
} else if (arg === "--check") {
  const bad = Object.entries(versions).filter(([, v]) => v !== current);
  if (bad.length) {
    for (const [file, v] of bad) console.error(`${file}: ${v} (expected ${current})`);
    console.error("Run `node scripts/version.mjs " + current + "` to synchronise.");
    process.exit(1);
  }
  console.log(`Version ${current} is in sync (${Object.keys(versions).length} places).`);
} else {
  const next = ["major", "minor", "patch"].includes(arg) ? bump(current, arg) : arg.replace(/^v/, "");
  if (!SEMVER.test(next)) {
    console.error(`Not a valid version: ${next}`);
    process.exit(2);
  }
  write(p("Cargo.toml"), read(p("Cargo.toml")).replace(workspaceRe, `$1${next}$3`));
  jsonVersion(p("app/package.json"), next);
  jsonVersion(p("app/package-lock.json"), next, 2); // root and packages[""]
  jsonVersion(p("app/src-tauri/tauri.conf.json"), next);
  // Rewrites only the workspace crates' versions in Cargo.lock.
  execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: root, stdio: "inherit" });
  console.log(`Version ${current} -> ${next}`);
}
