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
  folderId?: string | null;
}
export interface RequestFolder {
  id: string;
  name: string;
  kind: Request["kind"];
  parentId: string | null;
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
  requestFolders: RequestFolder[];
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
  folderId: null,
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
  const a = uid();
  return {
    id: uid(),
    name: "未命名流程",
    executionMode: "dataflow",
    nodes: [
      {
        id: a,
        type: "boost",
        position: { x: 80, y: 140 },
        data: { kind: "start", label: "开始", scope: "root" },
      },
    ],
    edges: [],
  };
};
export const initialWorkspace = (): Workspace => ({
  version: 1,
  requests: [],
  requestFolders: [],
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
  flow: "Flow 模块",
  evaluate: "Evaluate 求值",
  if: "条件分支",
  repeat: "Repeat 固定循环",
  while: "While 循环",
  foreach: "ForEach",
  constant: "常量",
  list: "List 数组",
  record: "Record 对象",
  select: "Select 取值",
  delay: "Delay 延迟",
  log: "Log 日志",
  display: "Display 展示",
  condition: "Condition 条件路由",
  validate: "Validate 校验",
  or: "OR 分支合流",
  collect: "Collect 收集",
  response: "Response 响应",
  output: "Output 输出",
  string: "String 字符串",
  bool: "Bool 布尔值",
  number: "Number 数值",
  null: "Null 空值",
  now: "Now 当前时间",
  date: "Date 日期",
  datetime: "Date & Time",
  createVariable: "Create Variable",
  getVariable: "Get Variable",
};
