import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
const script = fileURLToPath(
  new URL("../scripts/verify-bundles.mjs", import.meta.url),
);
const target = "x86_64-pc-windows-msvc";
function fixture(fn) {
  const root = mkdtempSync(join(tmpdir(), "boostman-bundles-"));
  try {
    const release = join(root, "src-tauri", "target", target, "release");
    mkdirSync(join(release, "bundle", "msi"), { recursive: true });
    mkdirSync(join(release, "bundle", "nsis"), { recursive: true });
    const pe = Buffer.alloc(128);
    pe.writeUInt32LE(64, 0x3c);
    pe.write("PE\0\0", 64, "ascii");
    pe.writeUInt16LE(0x8664, 68);
    writeFileSync(join(release, "boostman.exe"), pe);
    writeFileSync(join(release, "bundle", "msi", "test.msi"), "fixture MSI");
    writeFileSync(join(release, "bundle", "nsis", "test.exe"), "fixture NSIS");
    fn(root, release, pe);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}
test("checksums cover every required installer format", () =>
  fixture((root, release) => {
    const result = spawnSync(process.execPath, [script, "windows", target], {
      cwd: root,
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    const checksums = readFileSync(
      join(release, "bundle", "SHA256SUMS.txt"),
      "utf8",
    );
    assert.match(checksums, /^[a-f0-9]{64}  msi\/test.msi/m);
    assert.match(checksums, /^[a-f0-9]{64}  nsis\/test.exe/m);
    for (const [format, filename] of [
      ["msi", "test.msi"],
      ["nsis", "test.exe"],
    ]) {
      const directory = join(release, "bundle", format);
      const expectedHash = createHash("sha256")
        .update(readFileSync(join(directory, filename)))
        .digest("hex");
      assert.equal(
        readFileSync(join(directory, "SHA256SUMS.txt"), "utf8"),
        `${expectedHash}  ${filename}\n`,
        "Each separately downloaded artifact must have a standalone checksum manifest",
      );
    }
  }));
test("rejects ARM binaries, incomplete installers and unsupported target triples", () =>
  fixture((root, release, pe) => {
    const run = (triple) =>
      spawnSync(process.execPath, [script, "windows", triple], {
        cwd: root,
        encoding: "utf8",
      });
    pe.writeUInt16LE(0xaa64, 68);
    writeFileSync(join(release, "boostman.exe"), pe);
    assert.match(run(target).stderr, /not AMD64/);
    pe.writeUInt16LE(0x8664, 68);
    writeFileSync(join(release, "boostman.exe"), pe);
    rmSync(join(release, "bundle", "nsis", "test.exe"));
    assert.match(run(target).stderr, /No nsis installer/);
    assert.match(
      run("aarch64-pc-windows-msvc").stderr,
      /Only the configured x86_64 targets/,
    );
  }));
