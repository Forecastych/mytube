// Puts the official yt-dlp release where Tauri's externalBin expects it:
// src-tauri/binaries/yt-dlp-<target triple>.exe. Verified against the
// release's SHA2-256SUMS. Run by `npm run bundle`; skipped if already there
// (pass --force to refetch). The binary is not committed: it's ~18 MB and
// changes every few weeks.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
const ASSET = "yt-dlp.exe";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/^host: (\S+)$/m)?.[1];
if (!triple) throw new Error("Couldn't read the target triple from `rustc -vV`");
if (!triple.includes("windows")) throw new Error(`Only Windows builds bundle yt-dlp for now (host: ${triple})`);

const dir = join(root, "src-tauri", "binaries");
const out = join(dir, `yt-dlp-${triple}.exe`);
if (existsSync(out) && !process.argv.includes("--force")) {
  console.log(`yt-dlp: ${out} is there, skipping (--force to refetch)`);
  process.exit(0);
}

async function get(name) {
  const r = await fetch(`${RELEASE}/${name}`);
  if (!r.ok) throw new Error(`${name}: HTTP ${r.status}`);
  return Buffer.from(await r.arrayBuffer());
}

const sums = (await get("SHA2-256SUMS")).toString("utf8");
const expected = sums.match(new RegExp(`^([0-9a-f]{64})\\s+${ASSET.replace(".", "\\.")}$`, "m"))?.[1];
if (!expected) throw new Error(`${ASSET} is missing from SHA2-256SUMS`);

const exe = await get(ASSET);
const actual = createHash("sha256").update(exe).digest("hex");
if (actual !== expected) throw new Error(`${ASSET}: checksum mismatch (${actual} != ${expected})`);

mkdirSync(dir, { recursive: true });
writeFileSync(`${out}.part`, exe);
renameSync(`${out}.part`, out);
console.log(`yt-dlp: ${(exe.length / 1e6).toFixed(1)} MB, sha256 ok -> ${out}`);
