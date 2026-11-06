import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
import ts from "typescript";
const source = readFileSync(
  new URL("../src/model.ts", import.meta.url),
  "utf8",
);
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ES2022,
  },
}).outputText;
const { newConnection, changeConnectionDriver, databaseDrivers } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);
test("database switching uses shared defaults and preserves connection identity", () => {
  let config = {
    ...newConnection(),
    host: "db.internal",
    database: "sample",
    username: "user",
    secretId: "existing-password",
  };
  for (const driver of ["oracle", "mysql", "postgres"]) {
    const old = config;
    config = changeConnectionDriver(config, driver);
    assert.equal(config.port, databaseDrivers[driver].port);
    for (const key of [
      "id",
      "name",
      "host",
      "database",
      "username",
      "secretId",
    ])
      assert.equal(config[key], old[key]);
  }
});
test("custom ports and Oracle SID survive switching database type", () => {
  let config = { ...newConnection(), port: 15432, oracleMode: "sid" };
  for (const driver of ["oracle", "mysql", "postgres", "oracle"]) {
    config = changeConnectionDriver(config, driver);
    assert.equal(config.port, 15432);
    assert.equal(config.oracleMode, "sid");
  }
});
