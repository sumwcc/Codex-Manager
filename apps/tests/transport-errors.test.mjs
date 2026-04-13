import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "transport-errors.ts"
);

async function loadTransportErrorsModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-transport-errors-")
  );
  const tempFile = path.join(tempDir, "transport-errors.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const transportErrors = await loadTransportErrorsModule();

test("unwrapRpcPayload 统一解包 result 与 error envelope", () => {
  assert.equal(
    transportErrors.unwrapRpcPayload({ result: { ok: true, value: 42 } }).value,
    42
  );
  assert.throws(
    () => transportErrors.unwrapRpcPayload({ error: { message: "boom" } }),
    /boom/
  );
});

test("unwrapRpcPayload 会把 business error 直接提升为异常", () => {
  assert.throws(
    () =>
      transportErrors.unwrapRpcPayload({
        result: { ok: false, error: { message: "业务失败" } },
      }),
    /业务失败/
  );
});

test("getAppErrorMessage 与 isCommandMissingError 复用统一文案规则", () => {
  assert.equal(
    transportErrors.getAppErrorMessage({ ok: false, error: { message: "拒绝访问" } }),
    "拒绝访问"
  );
  assert.equal(
    transportErrors.getAppErrorMessage(new Error("unknown command: demo")),
    "unknown command: demo"
  );
  assert.equal(
    transportErrors.isCommandMissingError(new Error("service_demo is not a registered command")),
    true
  );
});
