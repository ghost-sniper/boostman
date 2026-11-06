import { useState, useCallback, useEffect } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  applyNodeChanges,
  applyEdgeChanges,
  addEdge,
} from "@xyflow/react";
import type { Connection, NodeChange, EdgeChange } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Field, RequestEditor } from "./components";
import { labels, uid, newRequest } from "./model";
import type { Data, Flow, Request } from "./model";
import type { RequestFolder } from "./model";
import { requestLabel } from "./requestFolders";
import { FlowBlockConfig } from "./FlowBlockConfig";
import { FlowDisplay } from "./FlowDisplay";
import { blockGroups, branchOptions, branchNodeIds } from "./flowCatalog";
import { FlowNode } from "./FlowNode";
import { ports, canConnect, variableColor } from "./flowPorts";
const nodeTypes = { boost: FlowNode };
export function FlowEditor({
  flow,
  onChange,
  requests,
  requestFolders = [],
  connections,
  statuses,
  results = {},
  disabled = false,
  flows = [],
}: {
  flow: Flow;
  onChange: (v: Flow) => void;
  requests: Request[];
  requestFolders?: RequestFolder[];
  connections: Data[];
  statuses: Record<string, string>;
  results?: Record<string, Data>;
  disabled?: boolean;
  flows?: Flow[];
}) {
  const [scope, setScope] = useState("root"),
    [selection, setSelection] = useState("");
  const [instance, setInstance] = useState<any>(null);
  const [group, setGroup] = useState("all");
  const [picker, setPicker] = useState(false),
    [blockSearch, setBlockSearch] = useState(""),
    [settings, setSettings] = useState(false);
  const graph = flow.executionMode === "dataflow";
  const node = flow.nodes.find((n) => n.id === selection),
    d = node?.data;
  const editNode = (id: string, patch: Data) => {
    if (disabled) return;
    const previous = flow.nodes.find((n) => n.id === id)?.data;
    const removed = new Set<string>();
    if (patch.conditions)
      for (const c of (previous?.conditions as Data[]) || [])
        if (!patch.conditions.some((next: Data) => next.id === c.id))
          removed.add(`${id}:case-${c.id}`);
    const ids = branchNodeIds(flow.nodes, removed);
    const changed = flow.nodes
      .filter((n) => !ids.has(n.id))
      .map((n) => (n.id === id ? { ...n, data: { ...n.data, ...patch } } : n))
      .map((n) =>
        previous?.kind === "createVariable" &&
        patch.variableName !== undefined &&
        n.data.variableRef === id
          ? { ...n, data: { ...n.data, variableName: patch.variableName } }
          : n,
      );
    onChange({
      ...flow,
      edges: flow.edges.filter(
        (e) =>
          !ids.has(e.source) &&
          !ids.has(e.target) &&
          (!graph ||
            (ports(
              changed.find((n) => n.id === e.source)!.data,
              "outputs",
            ).some((p) => p.id === e.sourceHandle) &&
              ports(
                changed.find((n) => n.id === e.target)!.data,
                "inputs",
              ).some((p) => p.id === e.targetHandle))),
      ),
      nodes: changed,
    });
  };
  const setData = (patch: Data) => editNode(selection, patch);
  const visible: typeof flow.nodes = flow.nodes
    .filter((n) => graph || (n.data.scope || "root") === scope)
    .map((n) => ({
      ...n,
      dragHandle: ".flow-block-header",
      data: {
        ...n.data,
        runStatus: statuses[n.id],
        preview: results[n.id]?.meta?.display,
        portMode: graph,
        requests,
        flows: flows.filter(
          (f) => f.id !== flow.id && f.executionMode === "dataflow",
        ),
        variableSources: flow.nodes
          .filter((n) => n.data.kind === "createVariable")
          .map((n) => ({
            id: n.id,
            name: n.data.variableName || n.data.label,
          })),
        runResult: results[n.id],
        onEdit: (patch: Data) => editNode(n.id, patch),
        onSettings: () => {
          setSelection(n.id);
          setSettings(true);
        },
        connectedInputs: Object.fromEntries(
          flow.edges
            .filter((e) => e.target === n.id)
            .map((e) => [
              e.targetHandle || "in",
              flow.nodes.find((n) => n.id === e.source)?.data.label || e.source,
            ]),
        ),
      },
      selected: n.id === selection,
      deletable: false,
    }));
  useEffect(() => {
    const frame = requestAnimationFrame(() =>
      instance?.fitView({ padding: 0.18, duration: 180, maxZoom: 1 }),
    );
    return () => cancelAnimationFrame(frame);
  }, [scope, visible.length, instance]);
  const scopes = [
    { id: "root", name: "主流程" },
    ...flow.nodes.flatMap((n) =>
      branchOptions(n.data).map((b) => ({
        id: `${n.id}:${b.id}`,
        name: `${n.data.label} / ${b.name}`,
      })),
    ),
  ];
  const add = (kind: string, position?: { x: number; y: number }) => {
    if (disabled) return;
    const id = uid(),
      end = visible.find((n) => n.data.kind === "end");
    const tail = graph
      ? visible[visible.length - 1]
      : visible.find(
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
        x: tail
          ? tail.position.x +
            (graph ? 400 : tail.data.kind === "display" ? 410 : 260)
          : 80,
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
        entries: [],
        value: null,
        path: "",
        displayMode: "json",
        xKey: "x",
        yKey: "y",
        level: "log",
        requestId: "",
        request: newRequest(kind === "sql" ? "sql" : "http"),
      },
    };
    if (kind === "delay") next.data.expression = "1000";
    const now = new Date();
    const defaults: Record<string, Data> = {
      string: { value: "" },
      bool: { value: false },
      number: { value: 0 },
      null: { value: null },
      date: { value: now.toISOString().slice(0, 10) },
      datetime: { value: now.toISOString() },
      condition: {
        conditions: [{ id: uid(), name: "条件 1", expression: "false" }],
      },
      validate: { schema: { type: "object" } },
      response: { statusCode: 200, headers: {} },
      createVariable: { variableName: "value" },
      getVariable: { variableName: "value" },
    };
    Object.assign(next.data, defaults[kind] || {});
    if (graph) {
      if (kind === "foreach") next.data.expression = "input";
      onChange({ ...flow, nodes: [...flow.nodes, next] });
      setSelection(id);
      setSettings(false);
      setPicker(false);
      return;
    }
    let edges = flow.edges;
    if (tail && end)
      edges = edges.filter(
        (e) => !(e.source === tail.id && e.target === end.id),
      );
    if (tail) edges = [...edges, { id: uid(), source: tail.id, target: id }];
    if (end) edges = [...edges, { id: uid(), source: id, target: end.id }];
    const nodes = flow.nodes.map((n) =>
      n.id === end?.id
        ? {
            ...n,
            position: {
              x: next.position.x + (kind === "display" ? 410 : 260),
              y: next.position.y,
            },
          }
        : n,
    );
    onChange({ ...flow, nodes: [...nodes, next], edges });
    setSelection(id);
  };
  const remove = () => {
    if (disabled) return;
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
    if (!graph && prev && next)
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
    (changes: NodeChange[]) => {
      if (disabled) return;
      const edits = changes.filter(
        (c) =>
          c.type === "position" || c.type === "add" || c.type === "replace",
      );
      if (!edits.length) return;
      onChange({
        ...flow,
        nodes: applyNodeChanges(edits, flow.nodes),
      });
    },
    [flow, onChange, disabled],
  );
  return (
    <div
      className={`flow-workspace ${graph ? "port-canvas" : "legacy-canvas"}`}
    >
      <div className="flow-tools">
        {!graph && (
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
        )}
        <span className="flow-mode-label">
          {graph ? "端口数据流" : "兼容模式 · 顺序链"}
        </span>
        <button
          className="block-picker-toggle"
          onClick={() => setPicker(!picker)}
        >
          ＋ Block
        </button>
        {picker && (
          <div className="flow-block-picker">
            <input
              aria-label="搜索组件"
              placeholder="搜索组件…"
              value={blockSearch}
              onChange={(e) => setBlockSearch(e.target.value)}
            />
            <div className="palette">
              <select
                aria-label="组件分类"
                value={group}
                onChange={(e) => setGroup(e.target.value)}
              >
                <option value="all">全部组件</option>
                {Object.entries(blockGroups).map(([key, g]) => (
                  <option key={key} value={key}>
                    {g.name}
                  </option>
                ))}
              </select>
              {(group === "all"
                ? Object.values(blockGroups).flatMap((g) => g.kinds)
                : blockGroups[group].kinds
              )
                .filter((k) => graph || k !== "flow")
                .filter(
                  (k) =>
                    labels[k]
                      .toLowerCase()
                      .includes(blockSearch.toLowerCase()) ||
                    k.toLowerCase().includes(blockSearch.toLowerCase()),
                )
                .map((k) => (
                  <button
                    key={k}
                    draggable
                    onDragStart={(e) => e.dataTransfer.setData("boost-node", k)}
                    onClick={() => add(k)}
                  >
                    ＋ {labels[k]}
                  </button>
                ))}
            </div>
          </div>
        )}
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
            // Capture the origin on press so the first move updates the position.
            nodeDragThreshold={0}
            nodesDraggable={!disabled}
            nodesConnectable={!disabled}
            nodes={visible}
            edges={[
              ...flow.edges.filter(
                (e) =>
                  visible.some((n) => n.id === e.source) &&
                  visible.some((n) => n.id === e.target),
              ),
              ...(graph
                ? flow.nodes
                    .filter(
                      (n) =>
                        n.data.kind === "getVariable" &&
                        n.data.variableRef &&
                        (selection === n.id ||
                          selection === n.data.variableRef),
                    )
                    .map((n) => ({
                      id: `reference-${n.id}`,
                      source: String(n.data.variableRef),
                      sourceHandle: "out",
                      target: n.id,
                      targetHandle: "variable-reference",
                      deletable: false,
                      selectable: false,
                      style: {
                        stroke: variableColor(
                          String(n.data.variableName || ""),
                        ),
                        strokeDasharray: "4 4",
                        strokeWidth: 1.2,
                      },
                    }))
                : []),
            ]}
            nodeTypes={nodeTypes}
            onInit={setInstance}
            onNodesChange={onNodesChange}
            isValidConnection={(c) =>
              !disabled && (!graph || canConnect(flow.nodes, flow.edges, c))
            }
            onEdgesChange={(changes: EdgeChange[]) => {
              if (disabled) return;
              onChange({
                ...flow,
                edges: applyEdgeChanges(changes, flow.edges),
              });
            }}
            onConnect={(c: Connection) =>
              !disabled && onChange({ ...flow, edges: addEdge(c, flow.edges) })
            }
            onNodeClick={(_, n) => setSelection(n.id)}
            onPaneClick={() => setSelection("")}
            fitView
            fitViewOptions={{ maxZoom: 1, padding: 0.18 }}
            defaultEdgeOptions={{
              type: "default",
              style: { stroke: "#a0a0a0", strokeWidth: 1.25 },
            }}
            minZoom={0.2}
            deleteKeyCode={["Backspace", "Delete"]}
          >
            <Background gap={16} size={1} color="#e6e6e6" />
            <Controls />
            <MiniMap nodeColor="#d9e7e4" maskColor="#f7f9facc" />
          </ReactFlow>
          <div className="canvas-hint">
            拖动节点排列 · 连线控制执行顺序 · 选中连线后按 Delete 删除
          </div>
        </div>
        {node && d && (!graph || settings) ? (
          <aside className="node-config">
            <div className="panel-heading">
              <strong>{labels[String(d.kind)]}</strong>
              <button
                className="icon"
                onClick={() => {
                  setSettings(false);
                  if (!graph) setSelection("");
                }}
              >
                ×
              </button>
            </div>
            <Field
              label="节点名称"
              value={d.label}
              onChange={(label) => setData({ label })}
            />
            <p className="muted mono">ID: {node.id.slice(0, 8)}</p>
            <Field
              label="画布注释"
              value={d.note}
              onChange={(note) => setData({ note })}
            />
            {graph && (
              <p className="muted">
                通过卡片左侧输入端口、右侧输出端口连线。多个已连接输入全部到达后才执行；OR
                使用先到达的数据。
              </p>
            )}
            <FlowBlockConfig
              key={node.id}
              data={{ ...d, portMode: graph }}
              onChange={setData}
            />
            {results[node.id]?.meta?.validation && (
              <div className="validation-report">
                <strong>
                  {results[node.id].meta.validation.passed
                    ? "Pass：校验通过"
                    : "Fail：校验不通过"}
                </strong>
                {results[node.id].meta.validation.errors.map(
                  (e: Data, i: number) => (
                    <p key={i}>
                      {e.path || "/"}：{e.message}
                    </p>
                  ),
                )}
              </div>
            )}
            {results[node.id]?.meta?.display && (
              <FlowDisplay display={results[node.id].meta.display} />
            )}
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
            {!graph && branchOptions(d).length > 0 && (
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
        ) : !graph ? (
          <aside className="node-config empty-config">
            <span>◇</span>
            <h3>把请求串成流程</h3>
            <p>
              从上方添加组件，点击节点配置。条件和循环的内部步骤在各自的子流程中编辑。
            </p>
            <p>首版按顺序执行，运行前由 Rust 检查连接与循环。</p>
          </aside>
        ) : null}
      </div>
    </div>
  );
}
