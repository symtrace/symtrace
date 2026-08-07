import * as cp from "child_process";
import * as vscode from "vscode";
import { DiffOutput } from "./types";
import { SymtraceConfig } from "./config";
import { Logger } from "./logger";

export class SymtraceRunner {
  constructor(
    private binaryPath: string,
    private logger: Logger
  ) {}

  async run(
    repoPath: string,
    commitA: string,
    commitB: string,
    config: SymtraceConfig,
    token: vscode.CancellationToken
  ): Promise<DiffOutput | undefined> {
    const args = this.buildArgs(repoPath, commitA, commitB, config);
    this.logger.info(`Running: ${this.binaryPath} ${args.join(" ")}`);

    return new Promise((resolve) => {
      const proc = cp.spawn(this.binaryPath, args);

      let stdout = "";
      let stderr = "";

      proc.stdout.on("data", (data: Buffer) => {
        stdout += data.toString();
      });

      proc.stderr.on("data", (data: Buffer) => {
        stderr += data.toString();
      });

      const cancelListener = token.onCancellationRequested(() => {
        proc.kill();
        resolve(undefined);
      });

      proc.on("close", (code) => {
        cancelListener.dispose();
        if (code !== 0) {
          this.logger.error(`symtrace exited with code ${code}: ${stderr}`);
          vscode.window.showErrorMessage(
            `Symtrace failed (exit code ${code}): ${stderr.slice(0, 300) || "Unknown error"}`
          );
          resolve(undefined);
          return;
        }
        try {
          const result: DiffOutput = JSON.parse(stdout);
          const totalFiles = result.summary?.totalFiles ?? result.summary?.total_files ?? result.files.length;
          const totalMs = result.performance?.totalTimeMs ?? result.performance?.total_time_ms ?? 0;
          this.logger.info(
            `Analysis complete: ${totalFiles} files, ${totalMs.toFixed(1)}ms`
          );
          resolve(result);
        } catch (e) {
          this.logger.error(`Failed to parse symtrace output: ${e}`);
          vscode.window.showErrorMessage(
            "Failed to parse symtrace JSON output. Check Output panel for details."
          );
          resolve(undefined);
        }
      });

      proc.on("error", (err) => {
        cancelListener.dispose();
        this.logger.error(`Failed to spawn symtrace: ${err.message}`);
        vscode.window.showErrorMessage(
          `Failed to run symtrace: ${err.message}`
        );
        resolve(undefined);
      });
    });
  }

  private buildArgs(
    repoPath: string,
    commitA: string,
    commitB: string,
    config: SymtraceConfig
  ): string[] {
    const args = ["-r", repoPath];

    if (commitB === "STAGED") {
      args.push(commitA, "--staged");
    } else if (commitB === "WORKTREE") {
      args.push(commitA);
    } else if (commitA && commitB) {
      args.push(commitA, commitB);
    } else if (commitA) {
      args.push(commitA);
    }

    args.push("--format", "json");

    if (config.logicOnly) {
      args.push("--logic-only");
    }
    if (config.noIncremental) {
      args.push("--no-incremental");
    }
    if (config.maxFileSize !== 5_242_880) {
      args.push("--max-file-size", config.maxFileSize.toString());
    }
    if (config.maxAstNodes !== 200_000) {
      args.push("--max-ast-nodes", config.maxAstNodes.toString());
    }
    if (config.maxRecursionDepth !== 2_048) {
      args.push("--max-recursion-depth", config.maxRecursionDepth.toString());
    }
    if (config.parseTimeoutMs !== 2_000) {
      args.push("--parse-timeout-ms", config.parseTimeoutMs.toString());
    }

    return args;
  }
}
