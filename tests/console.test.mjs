import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const source = readFileSync(
  new URL("../src/console.ts", import.meta.url),
  "utf8",
);
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
  },
}).outputText;
const { consoleEntries } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);
test("console includes earlier nodes when the last node is End", () => {
  const entries = consoleEntries({
    events: [
      {
        type: "console",
        logId: "a",
        nodeId: "http",
        path: [0, 1],
        phase: "post",
        message: "tag Rust",
        level: "log",
      },
      { type: "step", nodeId: "end", result: { data: { passed: true } } },
    ],
  });
  assert.equal(entries.length, 1);
  assert.equal(entries[0].message, "tag Rust");
  assert.deepEqual(entries[0].path, [0, 1]);
});
test("final snapshot restores logs missing from the event buffer, including failed runs", () => {
  const entries = consoleEntries({
    status: "failed",
    events: [],
    result: {
      logs: [
        {
          logId: "a",
          nodeId: "sql",
          level: "error",
          message: "before timeout",
        },
      ],
    },
  });
  assert.equal(entries[0].message, "before timeout");
  assert.equal(entries[0].level, "error");
});
test("old request log strings remain readable", () => {
  assert.equal(
    consoleEntries({
      events: [
        { type: "step", nodeId: "old", result: { logs: ["legacy message"] } },
      ],
    })[0].message,
    "legacy message",
  );
});
