import { Handle, Position } from "@xyflow/react";
import type { NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";
import type { Data, Request } from "./model";
import { labels, uid } from "./model";
import { JsonEditor } from "./components";
import { ports, variableColor } from "./flowPorts";
import { FlowDisplay } from "./FlowDisplay";
import { displayModes } from "./flowPresentation";
const symbols: Record<string, string> = {
  start: "▶",
  end: "■",
  http: "▧",
  sql: "▤",
  evaluate: "‹/›",
  condition: "⑂",
  if: "⑂",
  validate: "✓",
  or: "OR",
  repeat: "↻",
  foreach: "⋯",
  collect: "☷",
  display: "▣",
  log: "≡",
  string: "Aa",
  number: "#",
  bool: "T/F",
  null: "ø",
  now: "◷",
  date: "▦",
  datetime: "◷",
  list: "[]",
  record: "{}",
  select: "⌕",
  createVariable: "◆",
  getVariable: "◇",
  response: "↩",
  output: "⇥",
  delay: "◷",
};
export function FlowNode({ data, selected }: NodeProps) {
  const d = data as Data,
    kind = String(d.kind),
    graph = !!d.portMode;
  const set = (patch: Data) => d.onEdit?.(patch);
  const entries: Data[] = d.entries || [],
    bindings: Data[] = d.inputBindings || [];
  const ins = graph
    ? ports(d, "inputs")
    : kind === "start"
      ? []
      : [{ id: "in", label: "Input" }];
  const outs = graph
    ? ports(d, "outputs")
    : kind === "end"
      ? []
      : [{ id: "out", label: "Result" }];
  const updateEntry = (i: number, patch: Data) =>
    set({ entries: entries.map((e, j) => (j === i ? { ...e, ...patch } : e)) });
  const exprKinds = [
    "evaluate",
    "if",
    "while",
    "delay",
    "createVariable",
    "collect",
    "output",
    "response",
  ];
  return (
    <div
      style={
        {
          "--variable-color": variableColor(d.variableName || ""),
        } as CSSProperties
      }
      className={`flow-block ${selected ? "selected" : ""} kind-${kind} ${d.runStatus || ""}`}
    >
      {d.note && <div className="flow-block-note">{d.note}</div>}
      <div className="flow-block-header">
        <span className="block-glyph">{symbols[kind] || "◇"}</span>
        <span className="block-title" title={d.label || labels[kind]}>
          {d.label || labels[kind]}
        </span>
        <span className={`block-status ${d.runStatus || ""}`} />
        {kind === "display" && (
          <select
            className="nodrag nopan"
            aria-label="展示格式"
            value={d.displayMode || "json"}
            onChange={(e) => set({ displayMode: e.target.value })}
          >
            {Object.entries(displayModes).map(([v, t]) => (
              <option key={v} value={v}>
                {t}
              </option>
            ))}
          </select>
        )}
        <button
          className="nodrag nopan block-settings"
          title="组件设置"
          onClick={() => d.onSettings?.()}
        >
          ⚙
        </button>
      </div>
      <div className="flow-block-body nodrag nopan nowheel">
        {graph && kind === "getVariable" && (
          <Handle
            id="variable-reference"
            type="target"
            position={Position.Left}
            isConnectable={false}
            style={{ opacity: 0, pointerEvents: "none", top: 14 }}
          />
        )}
        {ins.map((p) => {
          const entryIndex = entries.findIndex(
              (e, i) => `item:${e.id || i}` === p.id,
            ),
            binding = bindings.find((b) => `var:${b.id}` === p.id),
            connected = d.connectedInputs?.[p.id];
          return (
            <div className="block-input-row" key={p.id}>
              <Handle
                type="target"
                position={Position.Left}
                id={graph ? p.id : undefined}
              />
              <span className="block-port-name">{p.label}</span>
              {connected ? (
                <div className="block-bound-value">
                  <small>↳ {connected}</small>
                  {graph && (
                    <input
                      aria-label={`${p.label} 输入路径`}
                      placeholder="/body/field（可选）"
                      value={d.inputPaths?.[p.id] || ""}
                      onChange={(e) =>
                        set({
                          inputPaths: {
                            ...d.inputPaths,
                            [p.id]: e.target.value,
                          },
                        })
                      }
                    />
                  )}
                </div>
              ) : entryIndex >= 0 ? (
                <div className="block-entry-value">
                  {kind === "record" && (
                    <input
                      aria-label={`字段名 ${entryIndex + 1}`}
                      value={entries[entryIndex].key}
                      onChange={(e) =>
                        updateEntry(entryIndex, { key: e.target.value })
                      }
                    />
                  )}
                  <input
                    aria-label={`值表达式 ${entryIndex + 1}`}
                    value={entries[entryIndex].expression}
                    placeholder="FQL value"
                    onChange={(e) =>
                      updateEntry(entryIndex, { expression: e.target.value })
                    }
                  />
                </div>
              ) : binding ? (
                <input
                  aria-label={`${p.label} 默认值`}
                  value={binding.expression || "null"}
                  onChange={(e) =>
                    set({
                      inputBindings: bindings.map((b) =>
                        b.id === binding.id
                          ? { ...b, expression: e.target.value }
                          : b,
                      ),
                    })
                  }
                />
              ) : (
                <span className="port-type">
                  {kind === "or" ? "any" : "{ }"}
                </span>
              )}
            </div>
          );
        })}
        {kind === "start" && (
          <details className="block-inline-settings">
            <summary>Takes input of any type</summary>
            <JsonEditor
              label="运行输入"
              rows={3}
              value={d.value ?? null}
              onChange={(value) => set({ value })}
            />
          </details>
        )}
        {kind === "string" && (
          <div className="block-literal">
            <span>Aa</span>
            <textarea
              aria-label="字符串内容"
              rows={1}
              value={d.value ?? ""}
              placeholder="Enter text…"
              onChange={(e) => set({ value: e.target.value })}
            />
          </div>
        )}
        {kind === "number" && (
          <JsonEditor
            label="Number"
            rows={1}
            value={d.value}
            onChange={(value) => set({ value })}
          />
        )}
        {kind === "bool" && (
          <select
            aria-label="Bool"
            value={String(d.value)}
            onChange={(e) => set({ value: e.target.value === "true" })}
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        )}
        {kind === "null" && <div className="block-null">null</div>}
        {kind === "constant" && (
          <JsonEditor
            label="JSON"
            rows={3}
            value={d.value}
            onChange={(value) => set({ value })}
          />
        )}
        {kind === "date" && (
          <input
            aria-label="Date"
            type="date"
            value={d.value || ""}
            onChange={(e) => set({ value: e.target.value })}
          />
        )}
        {kind === "datetime" && (
          <input
            aria-label="Date & Time"
            value={d.value || ""}
            onChange={(e) => set({ value: e.target.value })}
          />
        )}
        {kind === "now" && (
          <div className="block-null">Current Unix time · milliseconds</div>
        )}
        {kind === "select" && (
          <input
            aria-label="Select 路径"
            placeholder="Enter path… /body/id"
            value={d.path || ""}
            onChange={(e) => set({ path: e.target.value })}
          />
        )}
        {["createVariable", "getVariable"].includes(kind) && (
          <>
            {kind === "getVariable" && graph && (
              <select
                aria-label="变量来源"
                value={d.variableRef || ""}
                onChange={(e) =>
                  set({
                    variableRef: e.target.value,
                    variableName:
                      d.variableSources.find(
                        (v: Data) => v.id === e.target.value,
                      )?.name || d.variableName,
                  })
                }
              >
                <option value="">按名称读取</option>
                {(d.variableSources || []).map((v: Data) => (
                  <option key={v.id} value={v.id}>
                    {v.name}
                  </option>
                ))}
              </select>
            )}
            <input
              className="block-variable"
              aria-label="变量名称"
              value={d.variableName || ""}
              readOnly={kind === "getVariable" && !!d.variableRef}
              placeholder="variable"
              onChange={(e) => set({ variableName: e.target.value })}
            />
          </>
        )}
        {kind === "flow" && (
          <select
            aria-label="调用的 Flow"
            value={d.flowId || ""}
            onChange={(e) => set({ flowId: e.target.value })}
          >
            <option value="">选择包含 Output 的 Flow…</option>
            {(d.flows || []).map((f: Data) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        )}
        {["http", "sql"].includes(kind) && (
          <div className="block-request">
            <select
              aria-label="请求来源"
              value={d.requestId || ""}
              onChange={(e) => set({ requestId: e.target.value })}
            >
              <option value="">节点内配置</option>
              {(d.requests || [])
                .filter((r: Request) => r.kind === kind)
                .map((r: Request) => (
                  <option key={r.id} value={r.id}>
                    {r.name}
                  </option>
                ))}
            </select>
            <small>
              {kind === "http"
                ? `${d.request?.method || "GET"} ${d.request?.url || ""}`
                : d.request?.sql || "SQL Query"}
            </small>
            <button onClick={() => d.onSettings?.()}>编辑请求</button>
          </div>
        )}
        {exprKinds.includes(kind) && (
          <div className="block-expression">
            <small>{kind === "delay" ? "Delay · ms" : "FQL"}</small>
            <textarea
              aria-label={`${labels[kind]} 表达式`}
              rows={kind === "evaluate" ? 5 : 2}
              value={d.expression || ""}
              placeholder="Start writing FQL…"
              onChange={(e) => set({ expression: e.target.value })}
            />
          </div>
        )}
        {kind === "repeat" && (
          <div className="block-expression">
            <small>Count</small>
            <input
              aria-label="重复次数"
              value={d.expression || "3"}
              onChange={(e) => set({ expression: e.target.value })}
            />
          </div>
        )}
        {kind === "foreach" && (
          <div className="block-expression">
            <small>List (FQL fallback)</small>
            <input
              aria-label="数组表达式"
              value={d.expression || "input"}
              onChange={(e) => set({ expression: e.target.value })}
            />
          </div>
        )}
        {kind === "validate" && (
          <JsonEditor
            label="JSON Schema"
            rows={4}
            value={d.schema}
            onChange={(schema) => set({ schema })}
          />
        )}
        {kind === "condition" && (
          <div className="block-conditions">
            {(d.conditions || []).map((c: Data, i: number) => (
              <label key={c.id}>
                <span>{c.name || `Condition ${i + 1}`}</span>
                <input
                  aria-label={`条件 ${i + 1}`}
                  value={c.expression}
                  onChange={(e) =>
                    set({
                      conditions: d.conditions.map((v: Data) =>
                        v.id === c.id
                          ? { ...v, expression: e.target.value }
                          : v,
                      ),
                    })
                  }
                />
              </label>
            ))}
            <button
              onClick={() =>
                set({
                  conditions: [
                    ...(d.conditions || []),
                    {
                      id: uid(),
                      name: `Condition ${(d.conditions || []).length + 1}`,
                      expression: "false",
                    },
                  ],
                })
              }
            >
              ＋ Condition
            </button>
          </div>
        )}
        {kind === "response" && (
          <>
            <label className="block-mini-field">
              Status Code
              <input
                aria-label="Response 状态码"
                type="number"
                value={d.statusCode ?? 200}
                onChange={(e) => set({ statusCode: Number(e.target.value) })}
              />
            </label>
            <JsonEditor
              label="Headers"
              rows={2}
              value={d.headers || {}}
              onChange={(headers) => set({ headers })}
            />
          </>
        )}
        {["list", "record"].includes(kind) && (
          <button
            className="block-add-row"
            onClick={() =>
              set({
                entries: [
                  ...entries,
                  {
                    id: uid(),
                    key: `key${entries.length + 1}`,
                    expression: "null",
                  },
                ],
              })
            }
          >
            ＋ {kind === "list" ? "Item" : "Field"}
          </button>
        )}
        {graph &&
          ["evaluate", "http", "sql", "condition", "if"].includes(kind) && (
            <button
              className="block-add-row"
              onClick={() =>
                set({
                  inputBindings: [
                    ...bindings,
                    {
                      id: uid(),
                      name: `value${bindings.length + 1}`,
                      expression: "null",
                    },
                  ],
                })
              }
            >
              ＋ variable
            </button>
          )}
        {kind === "display" && (
          <div className="block-display">
            {d.preview ? (
              <FlowDisplay display={d.preview} compact />
            ) : (
              <p>Run your flow to see data</p>
            )}
          </div>
        )}
        {outs.map((p) => (
          <div className="block-output-row" key={p.id}>
            <span>{p.label}</span>
            <span className="port-type">
              {p.id === "finish" || p.id === "done" ? "◦" : "{ }"}
            </span>
            <Handle
              type="source"
              position={Position.Right}
              id={graph ? p.id : undefined}
            />
          </div>
        ))}
        {!!d.runResult && kind !== "display" && (
          <details className="block-result">
            <summary>Output · {d.runResult.status}</summary>
            <pre>
              {JSON.stringify(d.runResult.data, null, 2)?.slice(0, 4000)}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}
