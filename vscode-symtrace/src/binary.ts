import * as vscode from "vscode";
import * as cp from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import * as https from "https";
import { Logger } from "./logger";

const BINARY_NAME = os.platform() === "win32" ? "symtrace.exe" : "symtrace";
const GITHUB_REPO = "symtrace/symtrace";

export async function resolveBinary(
  context: vscode.ExtensionContext,
  logger: Logger
): Promise<string | undefined> {
  // Tier 1: Explicit config path
  const configPath = vscode.workspace
    .getConfiguration("symtrace")
    .get<string>("binaryPath");
  if (configPath && configPath.length > 0 && fs.existsSync(configPath)) {
    logger.info(`Using configured binary: ${configPath}`);
    return configPath;
  }

  // Tier 2: PATH lookup (cargo install symtrace)
  const pathBinary = await findInPath();
  if (pathBinary) {
    logger.info(`Found binary in PATH: ${pathBinary}`);
    return pathBinary;
  }

  // Tier 3: Previously downloaded binary
  const storagePath = context.globalStorageUri.fsPath;
  const cachedBinary = path.join(storagePath, BINARY_NAME);
  if (fs.existsSync(cachedBinary)) {
    logger.info(`Using cached binary: ${cachedBinary}`);
    return cachedBinary;
  }

  // Tier 4: Download from GitHub releases
  const autoDownload = vscode.workspace
    .getConfiguration("symtrace")
    .get<boolean>("autoDownloadBinary", true);
  if (!autoDownload) {
    logger.warn("Auto-download disabled and no binary found");
    return undefined;
  }

  const proceed = await vscode.window.showInformationMessage(
    "Symtrace binary not found. Download from GitHub releases?",
    "Download",
    "Install via cargo",
    "Cancel"
  );

  if (proceed === "Install via cargo") {
    vscode.window.showInformationMessage(
      'Run "cargo install symtrace" in your terminal, then reload VS Code.'
    );
    return undefined;
  }

  if (proceed !== "Download") {
    return undefined;
  }

  return downloadBinary(storagePath, logger);
}

async function findInPath(): Promise<string | undefined> {
  const cmd = os.platform() === "win32" ? "where" : "which";
  return new Promise((resolve) => {
    cp.exec(`${cmd} symtrace`, (err, stdout) => {
      if (err || !stdout.trim()) {
        resolve(undefined);
      } else {
        resolve(stdout.trim().split("\n")[0].trim());
      }
    });
  });
}

function getTargetTriple(): string | undefined {
  const platform = os.platform();
  const arch = os.arch();
  const map: Record<string, Record<string, string>> = {
    win32: { x64: "x86_64-pc-windows-msvc" },
    linux: {
      x64: "x86_64-unknown-linux-gnu",
      arm64: "aarch64-unknown-linux-gnu",
    },
    darwin: {
      x64: "x86_64-apple-darwin",
      arm64: "aarch64-apple-darwin",
    },
  };
  return map[platform]?.[arch];
}

async function downloadBinary(
  storagePath: string,
  logger: Logger
): Promise<string | undefined> {
  const triple = getTargetTriple();
  if (!triple) {
    vscode.window.showErrorMessage(
      `Unsupported platform: ${os.platform()}-${os.arch()}. Install via "cargo install symtrace" instead.`
    );
    return undefined;
  }

  try {
    // Fetch latest release info
    const releaseInfo = await fetchJson(
      `https://api.github.com/repos/${GITHUB_REPO}/releases/latest`
    );

    if (!releaseInfo || !releaseInfo.assets) {
      vscode.window.showErrorMessage(
        'No GitHub releases found. Install via "cargo install symtrace".'
      );
      return undefined;
    }

    // Find matching asset
    const asset = releaseInfo.assets.find(
      (a: { name: string }) =>
        a.name.includes(triple) &&
        (a.name.endsWith(".zip") || a.name.endsWith(".tar.gz"))
    );

    if (!asset) {
      vscode.window.showErrorMessage(
        `No pre-built binary for ${triple}. Install via "cargo install symtrace".`
      );
      return undefined;
    }

    // Download
    logger.info(`Downloading ${asset.name} from ${asset.browser_download_url}`);

    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Symtrace: Downloading binary...",
        cancellable: false,
      },
      async () => {
        if (!fs.existsSync(storagePath)) {
          fs.mkdirSync(storagePath, { recursive: true });
        }

        const archivePath = path.join(storagePath, asset.name);
        await downloadFile(asset.browser_download_url, archivePath);

        // Extract
        const binaryDest = path.join(storagePath, BINARY_NAME);
        if (asset.name.endsWith(".zip")) {
          // Use unzip on Unix, PowerShell on Windows
          if (os.platform() === "win32") {
            await execAsync(
              `powershell -Command "Expand-Archive -Path '${archivePath}' -DestinationPath '${storagePath}' -Force"`
            );
          } else {
            await execAsync(`unzip -o "${archivePath}" -d "${storagePath}"`);
          }
        } else {
          await execAsync(
            `tar -xzf "${archivePath}" -C "${storagePath}"`
          );
        }

        // Cleanup archive
        if (fs.existsSync(archivePath)) {
          fs.unlinkSync(archivePath);
        }

        // Make executable on Unix
        if (os.platform() !== "win32" && fs.existsSync(binaryDest)) {
          fs.chmodSync(binaryDest, 0o755);
        }
      }
    );

    const binaryPath = path.join(storagePath, BINARY_NAME);
    if (fs.existsSync(binaryPath)) {
      logger.info(`Downloaded binary to: ${binaryPath}`);
      vscode.window.showInformationMessage("Symtrace binary downloaded successfully.");
      return binaryPath;
    }

    // The binary might be in a subdirectory after extraction
    const candidates = findBinaryRecursive(storagePath);
    if (candidates.length > 0) {
      const src = candidates[0];
      const dest = path.join(storagePath, BINARY_NAME);
      fs.copyFileSync(src, dest);
      if (os.platform() !== "win32") {
        fs.chmodSync(dest, 0o755);
      }
      logger.info(`Downloaded binary to: ${dest}`);
      vscode.window.showInformationMessage("Symtrace binary downloaded successfully.");
      return dest;
    }

    vscode.window.showErrorMessage("Failed to locate symtrace binary after extraction.");
    return undefined;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    logger.error(`Download failed: ${msg}`);
    vscode.window.showErrorMessage(
      `Failed to download symtrace: ${msg}. Install via "cargo install symtrace".`
    );
    return undefined;
  }
}

function findBinaryRecursive(dir: string): string[] {
  const results: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...findBinaryRecursive(full));
    } else if (entry.name === BINARY_NAME) {
      results.push(full);
    }
  }
  return results;
}

interface GitHubRelease {
  assets: Array<{ name: string; browser_download_url: string }>;
}

function fetchJson(url: string): Promise<GitHubRelease | null> {
  return new Promise((resolve) => {
    const get = (u: string) => {
      https.get(
        u,
        { headers: { "User-Agent": "symtrace-vscode" } },
        (res) => {
          if (
            res.statusCode &&
            res.statusCode >= 300 &&
            res.statusCode < 400 &&
            res.headers.location
          ) {
            get(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            resolve(null);
            return;
          }
          let data = "";
          res.on("data", (chunk) => (data += chunk));
          res.on("end", () => {
            try {
              resolve(JSON.parse(data) as GitHubRelease);
            } catch {
              resolve(null);
            }
          });
        }
      ).on("error", () => resolve(null));
    };
    get(url);
  });
}

function downloadFile(url: string, dest: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const get = (u: string) => {
      https.get(
        u,
        { headers: { "User-Agent": "symtrace-vscode" } },
        (res) => {
          if (
            res.statusCode &&
            res.statusCode >= 300 &&
            res.statusCode < 400 &&
            res.headers.location
          ) {
            get(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            reject(new Error(`HTTP ${res.statusCode}`));
            return;
          }
          const file = fs.createWriteStream(dest);
          res.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve();
          });
          file.on("error", reject);
        }
      ).on("error", reject);
    };
    get(url);
  });
}

function execAsync(command: string): Promise<string> {
  return new Promise((resolve, reject) => {
    cp.exec(command, (err, stdout) => {
      if (err) {
        reject(err);
      } else {
        resolve(stdout);
      }
    });
  });
}
