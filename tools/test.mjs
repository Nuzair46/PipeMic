import ts from "typescript";
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const output = mkdtempSync(path.join(tmpdir(), "pipemic-tests-"));
try {
  const files = readdirSync(path.join(root, "tests")).filter(file => file.endsWith(".test.ts"));
  const program = ts.createProgram(files.map(file => path.join(root, "tests", file)), {
    target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS, moduleResolution: ts.ModuleResolutionKind.Node10,
    esModuleInterop: true, strict: true, skipLibCheck: true, resolveJsonModule: true, rootDir: root, outDir: output,
  });
  const diagnostics = ts.getPreEmitDiagnostics(program);
  if (diagnostics.length) {
    console.error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
      getCanonicalFileName: file => file, getCurrentDirectory: () => root, getNewLine: () => "\n",
    }));
    process.exitCode = 1;
  } else {
    program.emit();
    writeFileSync(path.join(output, "package.json"), '{"type":"commonjs"}');
    const result = spawnSync(process.execPath, ["--test", ...files.map(file => path.join(output, "tests", file.replace(/\.ts$/, ".js")))], { stdio: "inherit" });
    process.exitCode = result.status ?? 1;
  }
} finally {
  rmSync(output, { recursive: true, force: true });
}
