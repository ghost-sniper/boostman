import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const source = readFileSync(
  new URL("../src/requestFolders.ts", import.meta.url),
  "utf8",
);
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
  },
}).outputText;
const {
  folderPath,
  folderChoices,
  moveRequest,
  moveFolder,
  removeFolder,
  searchRequests,
} = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);
const sample = () => ({
  version: 1,
  requests: [
    { id: "h", name: "获取用户", kind: "http", folderId: "auth" },
    { id: "s", name: "获取用户", kind: "sql", folderId: "database" },
  ],
  requestFolders: [
    { id: "users", name: "用户组件", kind: "http", parentId: null },
    { id: "auth", name: "认证", kind: "http", parentId: "users" },
    { id: "database", name: "用户组件", kind: "sql", parentId: null },
  ],
  collections: [{ id: "c", items: [{ requestId: "h" }] }],
  flows: [{ id: "f", nodes: [{ data: { requestId: "h" } }] }],
  environments: [],
  connections: [],
});
test("separates HTTP and SQL folders while retaining nested component paths", () => {
  const w = sample();
  assert.equal(folderPath(w.requestFolders, "auth"), "用户组件 / 认证");
  assert.deepEqual(
    folderChoices(w.requestFolders, "http")
      .map((f) => f.id)
      .sort(),
    ["auth", "users"],
  );
  assert.deepEqual(folderChoices(w.requestFolders, "http", "users"), []);
  assert.deepEqual(
    folderChoices(w.requestFolders, "sql").map((f) => f.id),
    ["database"],
  );
});
test("moves requests without changing IDs or collection/flow references", () => {
  const w = sample(),
    moved = moveRequest(w, "h", "users");
  assert.equal(moved.requests[0].folderId, "users");
  assert.equal(moved.requests[0].id, "h");
  assert.deepEqual(moved.collections, w.collections);
  assert.deepEqual(moved.flows, w.flows);
  assert.equal(w.requests[0].folderId, "auth");
  assert.throws(() => moveRequest(w, "h", "database"), /同类型/);
  assert.equal(moveRequest(w, "h", null).requests[0].folderId, null);
});
test("prevents directory cycles and cross-type moves", () => {
  const w = sample();
  assert.throws(() => moveFolder(w, "users", "auth"), /自身子目录/);
  assert.throws(() => moveFolder(w, "auth", "database"), /跨类型/);
  assert.equal(moveFolder(w, "auth", null).requestFolders[1].parentId, null);
});
test("deleting a folder promotes contents and preserves requests", () => {
  const w = sample(),
    deleted = removeFolder(w, "users");
  assert.equal(
    deleted.requestFolders.find((f) => f.id === "auth").parentId,
    null,
  );
  assert.equal(deleted.requests[0].folderId, "auth");
  const removed = removeFolder(deleted, "auth");
  assert.equal(removed.requests[0].folderId, null);
  assert.equal(removed.requests.length, 2);
  assert.deepEqual(removed.flows, w.flows);
});
test("search returns matching descendants and ancestors without leaking other request types", () => {
  const w = sample();
  let found = searchRequests(w.requests, w.requestFolders, "http", "认证");
  assert.deepEqual([...found.requests], ["h"]);
  assert.deepEqual([...found.folders].sort(), ["auth", "users"]);
  found = searchRequests(w.requests, w.requestFolders, "sql", "用户组件");
  assert.deepEqual([...found.requests], ["s"]);
  assert.deepEqual([...found.folders], ["database"]);
});
