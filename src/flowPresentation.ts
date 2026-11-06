import type { Data } from "./model";
export const displayModes: Record<string, string> = {
  json: "JSON",
  text: "文本",
  table: "表格",
  boolean: "布尔值",
  line: "折线图",
};
export function flowResults(snapshot: Data | null): Record<string, Data> {
  const result: Record<string, Data> = {};
  for (const e of snapshot?.events || [])
    if (e.nodeId && e.result) result[e.nodeId] = e.result;
  // Final context survives the bounded event buffer, including a failing run.
  Object.assign(result, snapshot?.result?.context?.nodes || {});
  return result;
}
export function linePoints(value: unknown, xKey = "x", yKey = "y") {
  if (!Array.isArray(value)) return [];
  return value
    .map((v, i) => ({
      x: typeof v === "number" ? String(i + 1) : String(v?.[xKey] ?? i + 1),
      y: typeof v === "number" ? v : v?.[yKey],
    }))
    .filter((p) => typeof p.y === "number" && Number.isFinite(p.y));
}
