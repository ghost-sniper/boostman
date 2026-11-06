import type { Node, Edge } from "@xyflow/react";
export type Data = Record<string, any>;
export type DatabaseDriver = "postgres" | "mysql" | "oracle";
export interface DatabaseConnection extends Data {
  id: string;
  name: string;
  driver: DatabaseDriver;
  host: string;
  port: number;
  database: string;
  username: string;
  secretId: string;
  oracleMode: "service" | "sid";
}
export const databaseDrivers = {
  postgres: { name: "PostgreSQL", port: 5432 },
  mysql: { name: "MySQL", port: 3306 },
  oracle: { name: "Oracle", port: 1521 },
} as const;
export const newConnection = (): DatabaseConnection => {
  const id = uid();
  return {
    id,
    name: "新数据库连接",
    driver: "postgres",
    host: "localhost",
    port: databaseDrivers.postgres.port,
    database: "",
    username: "",
    secretId: `db-${id}`,
    oracleMode: "service",
  };
};
export function changeConnectionDriver(
  c: DatabaseConnection,
  driver: DatabaseDriver,
): DatabaseConnection {
  return {
    ...c,
    driver,
    port:
      c.port === databaseDrivers[c.driver].port
        ? databaseDrivers[driver].port
        : c.port,
    oracleMode: c.oracleMode || "service",
  };
}
export interface Request extends Data {
  id: string;
  name: string;
  kind: "http" | "sql";
}
export interface Collection extends Data {
  id: string;
  name: string;
  variables: Data;
  items: Data[];
}
export interface Flow extends Data {
  id: string;
  name: string;
  nodes: Node[];
  edges: Edge[];
}
export interface Workspace {
  version: number;
  requests: Request[];
  collections: Collection[];
  environments: Data[];
  connections: DatabaseConnection[];
  flows: Flow[];
}
export const uid = () => crypto.randomUUID();
export const newRequest = (kind: "http" | "sql" = "http"): Request => ({
  id: uid(),
  name: kind === "http" ? "未命名请求" : "SQL 查询",
  kind,
  method: "GET",
  url: "https://httpbin.org/get",
  query: [],
  headers: [],
  auth: { type: "none" },
  body: { type: "none", text: "", fields: [] },
  timeoutMs: 30000,
  followRedirects: true,
  sql: "SELECT 1 AS connected",
  params: [],
  connectionId: "",
  preScript: "",
  postScript: "",
});
export const newFlow = (): Flow => {
  const a = uid(),
    b = uid();
  return {
    id: uid(),
    name: "未命名流程",
    nodes: [
      {
        id: a,
        type: "boost",
        position: { x: 80, y: 140 },
        data: { kind: "start", label: "开始", scope: "root" },
      },
      {
        id: b,
        type: "boost",
        position: { x: 400, y: 140 },
        data: { kind: "end", label: "结束", scope: "root" },
      },
    ],
    edges: [{ id: uid(), source: a, target: b }],
  };
};
export const initialWorkspace = (): Workspace => ({
  version: 1,
  requests: [],
  collections: [],
  connections: [],
  environments: [],
  flows: [],
});
export const labels: Record<string, string> = {
  start: "开始",
  end: "结束",
  http: "HTTP 请求",
  sql: "SQL 请求",
  evaluate: "FQL 转换",
  if: "条件分支",
  repeat: "固定循环",
  while: "While 循环",
  foreach: "ForEach",
};
