import { useState, useCallback, useEffect } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Handle,
  Position,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
} from "@xyflow/react";
import type {
  NodeProps,
  Connection,
  NodeChange,
  EdgeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Field, RequestEditor } from "./components";
import { labels, uid, newRequest } from "./model";
import type { Data, Flow, Request } from "./model";
import type { RequestFolder } from "./model";
import { requestLabel } from "./requestFolders";
function BoostNode({ data, selected }: NodeProps) {
  const kind = String(data.kind);
  return (
    <div
      className={`flow-node ${selected ? "selected" : ""} ${String(data.runStatus || "")} kind-${kind}`}
    >
      <Handle type="target" position={Position.Left} />
      <span className="node-symbol">
        {{
          start: "▶",
          end: "■",
          http: "↗",
          sql: "▤",
          evaluate: "ƒ",
          if: "⑂",
          repeat: "↻",
          while: "↻",
          foreach: "⋯",
        }[kind] || "◇"}
      </span>
      <div>
        <small>{labels[kind]}</small>
        <strong>{String(data.label || labels[kind])}</strong>
      </div>
      {!!data.runStatus && <i className="node-state" />}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
const nodeTypes = { boost: BoostNode };
export function FlowEditor({
  flow,
  onChange,
  requests,
  requestFolders = [],
  connections,
  statuses,
}: {
  flow: Flow;
  onChange: (v: Flow) => void;
  requests: Request[];
  requestFolders?: RequestFolder[];
  connections: Data[];
  statuses: Record<string, string>;
}) {
  const [scope, setScope] = useState("root"),
    [selection, setSelection] = useState("");
  const [instance, setInstance] = useState<any>(null);
  const node = flow.nodes.find((n) => n.id === selection),
    d = node?.data;
  const setData = (patch: Data) =>
    onChange({
      ...flow,
      nodes: flow.nodes.map((n) =>
        n.id === selection ? { ...n, data: { ...n.data, ...patch } } : n,
      ),
    });
  const visible: typeof flow.nodes = flow.nodes
    .filter((n) => (n.data.scope || "root") === scope)
    .map((n) => ({
      ...n,
      data: { ...n.data, runStatus: statuses[n.id] },
      selected: n.id === selection,
      deletable: false,
    }));
  useEffect(() => {
    const frame = requestAnimationFrame(() =>
      instance?.fitView({ padding: 0.18, duration: 180 }),
    );
    return () => cancelAnimationFrame(frame);
  }, [scope, visible.length, instance]);
  const scopes = [
    { id: "root", name: "主流程" },
    ...flow.nodes.flatMap((n) =>
      n.data.kind === "if"
        ? [
            { id: `${n.id}:then`, name: `${n.data.label} / 条件成立` },
            { id: `${n.id}:else`, name: `${n.data.label} / 否则` },
          ]
        : ["repeat", "while", "foreach"].includes(String(n.data.kind))
          ? [{ id: `${n.id}:body`, name: `${n.data.label} / 循环体` }]
          : [],
    ),
  ];
  const add = (kind: string, position?: { x: number; y: number }) => {
    const id = uid(),
      end = visible.find((n) => n.data.kind === "end");
    const tail = visible.find(
      (n) =>
        n.data.kind !== "end" &&
        !flow.edges.some(
          (e) => e.source === n.id && (!end || e.target !== end.id),
        ),
    );
    const next = {
      id,
      type: "boost",
      position: position || {
        x: tail ? tail.position.x + 260 : 80,
        y: tail?.position.y || 140,
      },
      data: {
        kind,
        label: labels[kind],
        scope,
        expression:
          kind === "foreach"
            ? "input.rows"
            : kind === "repeat"
              ? "3"
              : kind === "if" || kind === "while"
                ? "false"
                : "input",
        maxIterations: 1000,
        requestId: "",
        request: newRequest(kind === "sql" ? "sql" : "http"),
      },
    };
    let edges = flow.edges;
    if (tail && end)
      edges = edges.filter(
        (e) => !(e.source === tail.id && e.target === end.id),
      );
    if (tail) edges = [...edges, { id: uid(), source: tail.id, target: id }];
    if (end) edges = [...edges, { id: uid(), source: id, target: end.id }];
    const nodes = flow.nodes.map((n) =>
      n.id === end?.id
        ? { ...n, position: { x: next.position.x + 260, y: next.position.y } }
        : n,
    );
    onChange({ ...flow, nodes: [...nodes, next], edges });
    setSelection(id);
  };
  const remove = () => {
    if (!node || ["start", "end"].includes(String(d?.kind))) return;
    const ids = new Set([node.id]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const n of flow.nodes) {
        const owner = String(n.data.scope || "root").split(":")[0];
        if (ids.has(owner) && !ids.has(n.id)) {
          ids.add(n.id);
          changed = true;
        }
      }
    }
    const prev = flow.edges.find((e) => e.target === node.id),
      next = flow.edges.find((e) => e.source === node.id);
    let edges = flow.edges.filter(
      (e) => !ids.has(e.source) && !ids.has(e.target),
    );
    if (prev && next)
      edges = [
        ...edges,
        { id: uid(), source: prev.source, target: next.target },
      ];
    onChange({
      ...flow,
      nodes: flow.nodes.filter((n) => !ids.has(n.id)),
      edges,
    });
    setSelection("");
  };
  const onNodesChange = useCallback(
    (changes: NodeChange[]) =>
      onChange({
        ...flow,
        nodes: applyNodeChanges(
          changes.filter((c) => c.type !== "remove"),
          flow.nodes,
        ),
      }),
    [flow, onChange],
  );
  return (
    <div className="flow-workspace">
      <div className="flow-tools">
        <div className="scope">
          <span>子流程</span>
          <select
            value={scope}
            onChange={(e) => {
              setScope(e.target.value);
              setSelection("");
            }}
          >
            {scopes.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
        <div className="palette">
          {["http", "sql", "evaluate", "if", "repeat", "while", "foreach"].map(
            (k) => (
              <button
                key={k}
                draggable
                onDragStart={(e) => e.dataTransfer.setData("boost-node", k)}
                onClick={() => add(k)}
              >
                ＋ {labels[k]}
              </button>
            ),
          )}
        </div>
      </div>
      <div className="flow-content">
        <div
          className="canvas"
          onDragOver={(e) => {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
          }}
          onDrop={(e) => {
            e.preventDefault();
            const kind = e.dataTransfer.getData("boost-node");
            if (labels[kind])
              add(
                kind,
                instance?.screenToFlowPosition({ x: e.clientX, y: e.clientY }),
              );
          }}
        >
          <ReactFlow
            nodes={visible}
            edges={flow.edges.filter(
              (e) =>
                visible.some((n) => n.id === e.source) &&
                visible.some((n) => n.id === e.target),
            )}
            nodeTypes={nodeTypes}
            onInit={setInstance}
            onNodesChange={onNodesChange}
            onEdgesChange={(changes: EdgeChange[]) =>
              onChange({
                ...flow,
                edges: applyEdgeChanges(changes, flow.edges),
              })
            }
            onConnect={(c: Connection) =>
              onChange({ ...flow, edges: addEdge(c, flow.edges) })
            }
            onNodeClick={(_, n) => setSelection(n.id)}
            onPaneClick={() => setSelection("")}
            fitView
            minZoom={0.2}
            deleteKeyCode={["Backspace", "Delete"]}
          >
            <Background gap={22} color="#dce2e8" />
            <Controls />
            <MiniMap nodeColor="#d9e7e4" maskColor="#f7f9facc" />
          </ReactFlow>
          <div className="canvas-hint">
            拖动节点排列 · 连线控制执行顺序 · 选中连线后按 Delete 删除
          </div>
        </div>
        {node && d ? (
          <aside className="node-config">
            <div className="panel-heading">
              <strong>{labels[String(d.kind)]}</strong>
              <button className="icon" onClick={() => setSelection("")}>
                ×
              </button>
            </div>
            <Field
              label="节点名称"
              value={d.label}
              onChange={(label) => setData({ label })}
            />
            <p className="muted mono">ID: {node.id.slice(0, 8)}</p>
            {["if", "while", "repeat", "foreach", "evaluate"].includes(
              String(d.kind),
            ) && (
              <>
                <label className="field">
                  <span>
                    {d.kind === "repeat"
                      ? "次数表达式"
                      : d.kind === "foreach"
                        ? "数组表达式"
                        : "FQL 表达式"}
                  </span>
                  <textarea
                    className="code"
                    rows={4}
                    value={String(d.expression || "")}
                    onChange={(e) => setData({ expression: e.target.value })}
                  />
                </label>
                <small className="muted">
                  可用 input、variables、nodes、item、index。节点结果使用
                  nodes.节点ID.data；包含连字符的 ID 请用 $lookup(nodes, 'ID')。
                </small>
                {["while", "repeat", "foreach"].includes(String(d.kind)) && (
                  <Field
                    label="最大迭代次数"
                    type="number"
                    value={d.maxIterations}
                    onChange={(v) => setData({ maxIterations: Number(v) })}
                  />
                )}
                {d.kind === "evaluate" && (
                  <Field
                    label="写入局部变量（可选）"
                    value={d.assignTo}
                    onChange={(assignTo) => setData({ assignTo })}
                  />
                )}
              </>
            )}
            {["http", "sql"].includes(String(d.kind)) && (
              <>
                <label className="field">
                  <span>请求来源</span>
                  <select
                    value={String(d.requestId || "")}
                    onChange={(e) => setData({ requestId: e.target.value })}
                  >
                    <option value="">节点内配置</option>
                    {requests
                      .filter((r) => r.kind === d.kind)
                      .map((r) => (
                        <option key={r.id} value={r.id}>
                          {requestLabel(r, requestFolders)}
                        </option>
                      ))}
                  </select>
                </label>
                {!d.requestId && (
                  <RequestEditor
                    compact
                    request={d.request as Request}
                    onChange={(request) => setData({ request })}
                    connections={connections}
                  />
                )}
              </>
            )}
            {["if", "while", "repeat", "foreach"].includes(String(d.kind)) && (
              <div className="branch-buttons">
                {scopes
                  .filter((s) => s.id.startsWith(`${node.id}:`))
                  .map((s) => (
                    <button
                      key={s.id}
                      onClick={() => {
                        setScope(s.id);
                        setSelection("");
                      }}
                    >
                      编辑 {s.name.split(" / ")[1]} →
                    </button>
                  ))}
              </div>
            )}
            {!["start", "end"].includes(String(d.kind)) && (
              <button className="danger subtle" onClick={remove}>
                删除节点及其子流程
              </button>
            )}
          </aside>
        ) : (
          <aside className="node-config empty-config">
            <span>◇</span>
            <h3>把请求串成流程</h3>
            <p>
              从上方添加组件，点击节点配置。条件和循环的内部步骤在各自的子流程中编辑。
            </p>
            <p>首版按顺序执行，运行前由 Rust 检查连接与循环。</p>
          </aside>
        )}
      </div>
    </div>
  );
}
