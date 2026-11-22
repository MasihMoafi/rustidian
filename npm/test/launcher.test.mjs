// The npm launcher: a first run downloads rustidian-linux-x86_64.tar.gz and SHA256SUMS, verifies the
// SHA-256 checksum, extracts the bundle, and runs rustidian with arguments preserved; a bad checksum is refused.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const launcher = join(dirname(fileURLToPath(import.meta.url)), "../bin/rustidian.js");
const APP = '#!/bin/sh\necho "rustidian ran: $*"\nexit 7\n';
const sha = bytes => createHash("sha256").update(bytes).digest("hex");

function makeTarball() {
  const work = mkdtempSync(join(tmpdir(), "rustidian-tar-"));
  const staging = join(work, "bundle");
  mkdirSync(join(staging, "demo-vault"), { recursive: true });
  writeFileSync(join(staging, "rustidian"), APP);
  chmodSync(join(staging, "rustidian"), 0o755);
  writeFileSync(join(staging, "demo-vault", "Home.md"), "# Demo\n");
  const archive = join(work, "rustidian-linux-x86_64.tar.gz");
  const res = spawnSync("tar", ["-czf", archive, "-C", staging, "rustidian", "demo-vault"], { encoding: "utf8" });
  if (res.status !== 0) throw new Error(res.stderr);
  const bytes = readFileSync(archive);
  rmSync(work, { recursive: true, force: true });
  return bytes;
}

/** A stand-in for the GitHub release: rustidian-linux-x86_64.tar.gz and SHA256SUMS. */
async function release(broken = false) {
  const archiveBytes = makeTarball();
  const sum = broken ? "0".repeat(64) : sha(archiveBytes);
  const sumsText = `${sum}  rustidian-linux-x86_64.tar.gz\n`;
  const requests = [];
  const server = createServer((request, response) => {
    const name = request.url.slice(1);
    requests.push(name);
    if (name === "rustidian-linux-x86_64.tar.gz") return response.end(archiveBytes);
    if (name === "SHA256SUMS") return response.end(sumsText);
    return response.writeHead(404).end();
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  return { url: `http://127.0.0.1:${server.address().port}`, requests, close: () => server.close() };
}

function run(env, args = []) {
  return new Promise(resolve => {
    const local = { NO_PROXY: "127.0.0.1", no_proxy: "127.0.0.1" };
    const child = spawn(process.execPath, [launcher, ...args], { env: { ...process.env, ...local, ...env } });
    let out = "", err = "";
    child.stdout.on("data", chunk => (out += chunk));
    child.stderr.on("data", chunk => (err += chunk));
    child.on("close", status => resolve({ status, out, err }));
  });
}

test("a first run downloads, verifies, extracts, and runs rustidian; a second run downloads nothing", async t => {
  const home = mkdtempSync(join(tmpdir(), "rustidian-npm-"));
  const source = await release(false);
  t.after(() => { source.close(); rmSync(home, { recursive: true, force: true }); });
  const env = { RUSTIDIAN_NPM_HOME: home, RUSTIDIAN_NPM_RELEASE_URL: source.url, RUSTIDIAN_PLATFORM: "Linux-x64" };
  const first = await run(env, ["--version", "two words"]);
  assert.equal(first.out, "rustidian ran: --version two words\n");
  assert.equal(first.status, 7);
  assert.equal(source.requests.length, 2);
  assert.ok(existsSync(join(home, "demo-vault", "Home.md")));
  assert.equal(readFileSync(join(home, "rustidian"), "utf8"), APP);
  const second = await run(env, ["again"]);
  assert.equal(second.out, "rustidian ran: again\n");
  assert.equal(source.requests.length, 2, "nothing is downloaded twice");
});

test("an archive that does not match its published checksum is refused, and nothing is run", async t => {
  const home = mkdtempSync(join(tmpdir(), "rustidian-npm-"));
  const source = await release(true);
  t.after(() => { source.close(); rmSync(home, { recursive: true, force: true }); });
  const result = await run({ RUSTIDIAN_NPM_HOME: home, RUSTIDIAN_NPM_RELEASE_URL: source.url, RUSTIDIAN_PLATFORM: "Linux-x64" });
  assert.equal(result.status, 1);
  assert.equal(result.out, "");
  assert.match(result.err, /does not match its published SHA-256/);
  assert.equal(existsSync(join(home, ".complete")), false);
  assert.equal(existsSync(join(home, "rustidian")), false);
});

test("another platform is told that Rustidian publishes Linux x86_64 only", async () => {
  const result = await run({ RUSTIDIAN_PLATFORM: "Darwin-arm64" });
  assert.equal(result.status, 1);
  assert.match(result.err, /Linux x86_64 binary only \(detected Darwin-arm64\)/);
});
