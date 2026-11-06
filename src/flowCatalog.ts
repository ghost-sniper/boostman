import type { Data } from "./model";
export const blockGroups: Record<string, { name: string; kinds: string[] }> = {
  requests: { name: "请求", kinds: ["http", "sql", "flow"] },
  logic: {
    name: "逻辑与循环",
    kinds: [
      "condition",
      "validate",
      "evaluate",
      "if",
      "or",
      "repeat",
      "while",
      "foreach",
      "collect",
      "delay",
    ],
  },
  data: {
    name: "数据",
    kinds: [
      "constant",
      "string",
      "bool",
      "number",
      "null",
      "list",
      "record",
      "select",
    ],
  },
  dates: { name: "日期与时间", kinds: ["now", "date", "datetime"] },
  variables: { name: "变量", kinds: ["createVariable", "getVariable"] },
  outputs: {
    name: "输出与展示",
    kinds: ["response", "output", "display", "log"],
  },
};
export function branchOptions(data: Data): { id: string; name: string }[] {
  if (data.kind === "if")
    return [
      { id: "then", name: "条件成立" },
      { id: "else", name: "否则" },
    ];
  if (data.kind === "validate")
    return [
      { id: "pass", name: "Pass 校验通过" },
      { id: "fail", name: "Fail 校验失败" },
    ];
  if (data.kind === "condition")
    return [
      ...(data.conditions || []).map((c: Data, i: number) => ({
        id: `case-${c.id}`,
        name: c.name || `条件 ${i + 1}`,
      })),
      { id: "default", name: "Default 默认" },
    ];
  if (["repeat", "while", "foreach"].includes(data.kind))
    return [{ id: "body", name: "循环体" }];
  return [];
}
export function branchNodeIds(nodes: Data[], scopes: Set<string>): Set<string> {
  const ids = new Set<string>();
  let changed = true;
  while (changed) {
    changed = false;
    for (const n of nodes)
      if (scopes.has(n.data.scope || "root") && !ids.has(n.id)) {
        ids.add(n.id);
        for (const b of branchOptions(n.data)) scopes.add(`${n.id}:${b.id}`);
        changed = true;
      }
  }
  return ids;
}
