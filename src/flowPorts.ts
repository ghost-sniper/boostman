import spec from "./flowPorts.json";
import type { Data } from "./model";
export type Port = { id: string; label: string };
export function variableColor(name: string): string {
  let hash = 0;
  for (const c of name) hash = (hash * 31 + c.charCodeAt(0)) | 0;
  return ["#9462ad", "#bb8a37", "#4c92ab", "#5c9b79", "#b6718b"][
    Math.abs(hash) % 5
  ];
}
export function ports(data: Data, side: "inputs" | "outputs"): Port[] {
  const base =
    (spec as Record<string, { inputs: string[][]; outputs: string[][] }>)[
      data.kind
    ]?.[side] || [];
  const result = base.map(([id, label]) => ({ id, label }));
  if (side === "outputs" && data.kind === "condition")
    result.unshift(
      ...(data.conditions || []).map((c: Data, i: number) => ({
        id: `case-${c.id}`,
        label: c.name || `Condition ${i + 1}`,
      })),
    );
  if (side === "inputs") {
    if (["list", "record"].includes(data.kind))
      result.push(
        ...(data.entries || []).map((e: Data, i: number) => ({
          id: `item:${e.id || i}`,
          label: data.kind === "list" ? String(i) : e.key || `key${i + 1}`,
        })),
      );
    result.push(
      ...(data.inputBindings || []).map((b: Data) => ({
        id: `var:${b.id}`,
        label: b.name,
      })),
    );
  }
  return result;
}
export function canConnect(
  nodes: Data[],
  edges: Data[],
  connection: Data,
): boolean {
  const a = nodes.find((n) => n.id === connection.source),
    b = nodes.find((n) => n.id === connection.target);
  if (!a || !b || a.id === b.id) return false;
  if (
    !ports(a.data, "outputs").some((p) => p.id === connection.sourceHandle) ||
    !ports(b.data, "inputs").some((p) => p.id === connection.targetHandle)
  )
    return false;
  if (
    edges.some(
      (e) => e.target === b.id && e.targetHandle === connection.targetHandle,
    )
  )
    return false;
  const visit = [b.id],
    seen = new Set<string>();
  while (visit.length) {
    const id = visit.pop()!;
    if (id === a.id) return false;
    if (seen.has(id)) continue;
    seen.add(id);
    visit.push(...edges.filter((e) => e.source === id).map((e) => e.target));
  }
  return true;
}
