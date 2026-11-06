import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, basename } from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

const [platform, target] = process.argv.slice(2);
const targets = {
  windows: {
    triple: "x86_64-pc-windows-msvc",
    formats: { msi: ".msi", nsis: ".exe" },
  },
  linux: {
    triple: "x86_64-unknown-linux-gnu",
    formats: { deb: ".deb", rpm: ".rpm" },
  },
  macos: { triple: "x86_64-apple-darwin", formats: { dmg: ".dmg" } },
};
const config = targets[platform];
if (!config || target !== config.triple)
  throw new Error("Only the configured x86_64 targets are supported");
const release = join("src-tauri", "target", target, "release");
const bundle = join(release, "bundle");
const run = (command, args) =>
  execFileSync(command, args, { encoding: "utf8" }).trim();
// Inspect the application binary itself, rather than relying on filenames to establish architecture.
if (platform === "windows") {
  const exe = readFileSync(join(release, "boostman.exe"));
  const pe = exe.readUInt32LE(0x3c);
  if (
    exe.toString("ascii", pe, pe + 4) !== "PE\0\0" ||
    exe.readUInt16LE(pe + 4) !== 0x8664
  ) {
    throw new Error("The Windows binary is not AMD64");
  }
} else if (platform === "linux") {
  const elf = readFileSync(join(release, "boostman"));
  if (
    elf.toString("ascii", 1, 4) !== "ELF" ||
    elf[4] !== 2 ||
    elf[5] !== 1 ||
    elf.readUInt16LE(18) !== 62
  ) {
    throw new Error("The Linux binary is not x86_64 ELF");
  }
} else if (
  run("lipo", [
    "-archs",
    join(bundle, "macos", "boostman.app", "Contents", "MacOS", "boostman"),
  ]) !== "x86_64"
) {
  throw new Error("The macOS application must contain only x86_64");
}
const checksums = [];
for (const [format, extension] of Object.entries(config.formats)) {
  const directory = join(bundle, format);
  const files = readdirSync(directory).filter((name) =>
    name.endsWith(extension),
  );
  if (!files.length) throw new Error(`No ${format} installer was generated`);
  for (const name of files) {
    const path = join(directory, name);
    if (
      format === "deb" &&
      run("dpkg-deb", ["--field", path, "Architecture"]) !== "amd64"
    )
      throw new Error("DEB architecture is not amd64");
    if (
      format === "rpm" &&
      run("rpm", ["-qp", "--queryformat", "%{ARCH}", path]) !== "x86_64"
    )
      throw new Error("RPM architecture is not x86_64");
    const bytes = readFileSync(path);
    if (!bytes.length) throw new Error(`Empty installer: ${path}`);
    checksums.push(
      `${createHash("sha256").update(bytes).digest("hex")}  ${format}/${basename(path)}`,
    );
    console.log(`Verified ${path} (${bytes.length} bytes)`);
  }
}
writeFileSync(join(bundle, "SHA256SUMS.txt"), `${checksums.join("\n")}\n`);
