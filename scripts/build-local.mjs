import { spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { machine } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const npm = process.env.npm_execpath;

function run(command, args, capture = false) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: capture ? ["ignore", "pipe", "inherit"] : "inherit",
    encoding: "utf8",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.signal ?? result.status})`,
    );
  }
  return result.stdout;
}

try {
  if (!npm) throw new Error("Use npm run build:local to start the build.");
  const platforms = {
    win32: "pc-windows-msvc",
    linux: "unknown-linux-gnu",
    darwin: "apple-darwin",
  };
  const platform = platforms[process.platform];
  if (!platform) throw new Error(`Unsupported platform: ${process.platform}`);

  let hardware = (
    (process.platform === "win32" && process.env.PROCESSOR_ARCHITEW6432) ||
    machine()
  ).toLowerCase();
  if (process.platform === "darwin" && hardware === "x86_64") {
    // Under Rosetta, uname reports x86_64 even on Apple Silicon.
    const arm = spawnSync("sysctl", ["-n", "hw.optional.arm64"], {
      encoding: "utf8",
    });
    if (arm.status === 0 && arm.stdout.trim() === "1") hardware = "arm64";
  }
  const architectures = {
    x64: "x86_64",
    x86_64: "x86_64",
    amd64: "x86_64",
    arm64: "aarch64",
    aarch64: "aarch64",
  };
  const architecture = architectures[hardware];
  if (!architecture) throw new Error(`Unsupported architecture: ${hardware}`);
  const target = `${architecture}-${platform}`;

  console.log(`Building Boostman Release for ${target}`);
  run("rustup", ["target", "add", target]);
  run(process.execPath, [npm, "ci"]);
  const { target_directory } = JSON.parse(
    run(
      "cargo",
      [
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--locked",
      ],
      true,
    ),
  );
  run(process.execPath, [
    npm,
    "run",
    "tauri",
    "--",
    "build",
    "--ci",
    "--target",
    target,
    "--no-bundle",
    "--",
    "--locked",
  ]);

  const executable = process.platform === "win32" ? "boostman.exe" : "boostman";
  const output = join(target_directory, target, "release", executable);
  const file = statSync(output);
  if (!file.isFile() || file.size === 0)
    throw new Error(`Invalid build output: ${output}`);
  console.log(`Executable: ${output}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
