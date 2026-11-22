#!/usr/bin/env node
// `npx rustidian` or, after `npm install -g rustidian`, `rustidian`: launches the Rustidian desktop app.
// On its first run it downloads this version's portable release archive from GitHub, verifies its SHA-256
// against SHA256SUMS, and extracts it into one folder; later runs start immediately.
// RUSTIDIAN_NPM_HOME moves that folder; RUSTIDIAN_NPM_RELEASE_URL and RUSTIDIAN_PLATFORM exist for the tests.
"use strict";

const { spawnSync } = require("node:child_process");
const { createHash } = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { version } = require("../package.json");

const ASSET = "rustidian-linux-x86_64.tar.gz";
const SUMS = "SHA256SUMS";

const platform = process.env.RUSTIDIAN_PLATFORM || `${os.type()}-${os.arch()}`;
if (platform !== "Linux-x64") {
  process.stderr.write(
    `Rustidian publishes a Linux x86_64 binary only (detected ${platform}).\n` +
      "Build from source with `cargo build --release --locked`.\n",
  );
  process.exit(1);
}

const release =
  process.env.RUSTIDIAN_NPM_RELEASE_URL || `https://github.com/MasihMoafi/rustidian/releases/download/v${version}`;
const data = process.env.XDG_DATA_HOME || path.join(os.homedir(), ".local", "share");
const home = process.env.RUSTIDIAN_NPM_HOME || path.join(data, "rustidian-npm", version);

/**
 * Saves `url` to `file`. Prefers `curl` so proxy settings and resumed downloads work reliably; falls back to `fetch`.
 */
async function download(url, file) {
  const curl = spawnSync(
    "curl",
    [
      "--fail",
      "--location",
      "--silent",
      "--show-error",
      "--retry",
      "8",
      "--retry-delay",
      "2",
      "--retry-all-errors",
      "--continue-at",
      "-",
      "--output",
      file,
      url,
    ],
    { stdio: ["ignore", "ignore", "pipe"], encoding: "utf8" },
  );
  if (!curl.error) {
    if (curl.status !== 0) throw new Error(`${url}: ${curl.stderr.trim() || `curl exit ${curl.status}`}`);
    return;
  }
  if (curl.error.code !== "ENOENT") throw curl.error;
  const response = await fetch(url, { redirect: "follow" });
  if (!response.ok) throw new Error(`${url} answered ${response.status}`);
  fs.writeFileSync(file, Buffer.from(await response.arrayBuffer()));
}

function sha256(file) {
  const hash = createHash("sha256");
  const handle = fs.openSync(file, "r");
  try {
    const chunk = Buffer.alloc(1 << 20);
    for (let read; (read = fs.readSync(handle, chunk, 0, chunk.length)) > 0; ) hash.update(chunk.subarray(0, read));
  } finally {
    fs.closeSync(handle);
  }
  return hash.digest("hex");
}

function expectedSha256(sumsText, assetName) {
  for (const line of sumsText.split(/\r?\n/)) {
    const match = line.trim().match(/^([0-9a-fA-F]{64})\s+\*?(.+)$/);
    if (match && match[2].trim() === assetName) return match[1].toLowerCase();
  }
  throw new Error(`${SUMS} does not contain an entry for ${assetName}`);
}

async function install() {
  process.stderr.write(`Rustidian ${version}: first run, downloading to ${home}\n`);
  fs.mkdirSync(home, { recursive: true });
  const archive = path.join(home, `${ASSET}.installing`);
  const sums = path.join(home, `${SUMS}.installing`);
  const extractDir = path.join(home, ".extracting");
  fs.rmSync(sums, { force: true });
  fs.rmSync(extractDir, { recursive: true, force: true });

  try {
    await download(`${release}/${ASSET}`, archive);
    await download(`${release}/${SUMS}`, sums);
    const expected = expectedSha256(fs.readFileSync(sums, "utf8"), ASSET);
    const actual = sha256(archive);
    if (actual !== expected) {
      fs.rmSync(archive, { force: true });
      throw new Error(`${ASSET} does not match its published SHA-256 (expected ${expected}, got ${actual})`);
    }
    fs.mkdirSync(extractDir, { recursive: true });
    const untar = spawnSync("tar", ["-xzf", archive, "-C", extractDir], {
      stdio: ["ignore", "ignore", "pipe"],
      encoding: "utf8",
    });
    if (untar.error) throw untar.error;
    if (untar.status !== 0) throw new Error(`tar failed (${untar.status}): ${untar.stderr.trim()}`);
    const stagedBin = path.join(extractDir, "rustidian");
    if (!fs.existsSync(stagedBin)) throw new Error(`${ASSET} did not extract a rustidian executable`);
    fs.chmodSync(stagedBin, 0o755);
    for (const entry of fs.readdirSync(extractDir)) {
      const dest = path.join(home, entry);
      fs.rmSync(dest, { recursive: true, force: true });
      fs.renameSync(path.join(extractDir, entry), dest);
    }
    fs.writeFileSync(path.join(home, ".complete"), `${version}\n`);
  } finally {
    fs.rmSync(sums, { force: true });
    fs.rmSync(extractDir, { recursive: true, force: true });
  }
}

(async () => {
  if (!fs.existsSync(path.join(home, ".complete"))) {
    try {
      await install();
    } catch (error) {
      process.stderr.write(
        `Rustidian could not be installed: ${error.message}\nNothing was run. Try again, or see ${release}\n`,
      );
      process.exit(1);
    }
  }
  const result = spawnSync(path.join(home, "rustidian"), process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    process.stderr.write(`${result.error.message}\n`);
    process.exit(1);
  }
  process.exit(result.status === null ? 1 : result.status);
})();
