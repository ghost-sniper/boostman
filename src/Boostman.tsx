import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ValidationContext,
  Field,
  JsonEditor,
  RequestEditor,
} from "./components";
import { CollectionEditor } from "./CollectionEditor";
import { FlowEditor } from "./FlowEditor";
import { ConnectionEditor } from "./ConnectionEditor";
import { RequestSidebar } from "./RequestSidebar";
import { RequestFolderEditor } from "./RequestFolderEditor";
import {
  folderChoices,
  folderPath,
  moveRequest,
  moveFolder,
  removeFolder,
} from "./requestFolders";
import { ResponsePanel } from "./ResponsePanel";
import {
  initialWorkspace,
  newFlow,
  newRequest,
  newConnection,
  uid,
} from "./model";
import type {
  Data,
  Workspace,
  Request,
  Collection,
  Flow,
  DatabaseConnection,
} from "./model";
import "./App.css";
type Section =
  "requests" | "collections" | "flows" | "connections" | "environments";
const names: Record<Section, string> = {
  requests: "请求",
  collections: "集合",
  flows: "Flows",
  connections: "数据库",
  environments: "环境与凭据",
};
export default function Boostman() {
  const [requestKind, setRequestKind] = useState<Request["kind"]>("http");
  const [activeFolderId, setActiveFolderId] = useState<string | null>(null);
  const [runTarget, setRunTarget] = useState("");
  const [workspace, setWorkspace] = useState<Workspace>(initialWorkspace),
    [section, setSection] = useState<Section>("requests"),
    [selected, setSelected] = useState(""),
    [search, setSearch] = useState("");
  const [loaded, setLoaded] = useState(false),
    [dirty, setDirty] = useState(false),
    [message, setMessage] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [help, setHelp] = useState(false);
  const [environmentId, setEnvironmentId] = useState(""),
    [options, setOptions] = useState<Data>({
      iterations: 1,
      data: [{}],
      continueOnError: false,
      scriptTimeoutMs: 5000,
    });
  const [snapshot, setSnapshot] = useState<Data | null>(null),
    [runId, setRunId] = useState(""),
    [secretId, setSecretId] = useState(""),
    [secretValue, setSecretValue] = useState("");
  const errors = useRef<Record<string, string>>({});
  const report = useCallback((id: string, value: string) => {
    if (value) errors.current[id] = value;
    else delete errors.current[id];
  }, []);
  const running = snapshot?.status === "running" || busy;
  const native = isTauri();
  useEffect(() => {
    if (!native) {
      setLoaded(true);
      setMessage("浏览器仅用于界面预览。请通过桌面应用执行和保存。");
      return;
    }
    invoke<Workspace>("load_workspace")
      .then((w) => {
        setWorkspace(w);
        setSelected(w.requests[0]?.id || "");
        setRequestKind(w.requests[0]?.kind || "http");
        setActiveFolderId(w.requests[0]?.folderId || null);
        setLoaded(true);
      })
      .catch((e) => {
        setError(String(e));
        setLoaded(true);
      });
  }, []);
  useEffect(() => {
    if (!runId) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let refreshing = false;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      try {
        const s = await invoke<Data>("get_run", { runId });
        if (disposed) return;
        setSnapshot(s);
        if (s.status !== "running") {
          clearInterval(timer);
          const w = await invoke<Workspace>("load_workspace");
          if (!disposed) {
            setWorkspace(w);
            setDirty(false);
          }
        }
      } catch (e) {
        if (!disposed) setError(String(e));
      } finally {
        refreshing = false;
      }
    };
    const timer = setInterval(refresh, 600);
    listen<Data>("run-event", (e) => {
      if (e.payload.runId === runId) void refresh();
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((e) => setError(String(e)));
    void refresh();
    return () => {
      disposed = true;
      clearInterval(timer);
      unlisten?.();
    };
  }, [runId]);
  const mutate = (next: Workspace) => {
    setWorkspace(next);
    setDirty(true);
    setMessage("");
  };
  const items = workspace[section] as Data[];
  const current = items.find((i) => i.id === selected);
  const update = (value: Data) =>
    mutate({
      ...workspace,
      [section]: items.map((i) => (i.id === selected ? value : i)),
    });
  const choose = (s: Section) => {
    setSection(s);
    const first =
      s === "requests"
        ? workspace.requests.find((r) => r.kind === requestKind)
        : workspace[s][0];
    setSelected(first?.id || "");
    if (s === "requests") setActiveFolderId(first?.folderId || null);
    setSearch("");
    setHelp(false);
  };
  const add = (kind?: "http" | "sql") => {
    let value: Data;
    switch (section) {
      case "requests":
        value = {
          ...newRequest(kind || requestKind),
          folderId: !kind || kind === requestKind ? activeFolderId : null,
        };
        setRequestKind(kind || requestKind);
        setActiveFolderId(value.folderId);
        break;
      case "collections":
        value = {
          id: uid(),
          name: "新集合",
          variables: {},
          items: [],
          preScript: "",
          postScript: "",
        };
        break;
      case "flows":
        value = newFlow();
        break;
      case "connections":
        value = newConnection();
        break;
      default:
        value = { id: uid(), name: "新环境", variables: {} };
    }
    mutate({ ...workspace, [section]: [...items, value] });
    setSelected(value.id);
  };
  const activeFolder = workspace.requestFolders.find(
    (f) => f.id === activeFolderId && f.kind === requestKind,
  );
  const organize = (change: () => Workspace) => {
    try {
      mutate(change());
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };
  const switchKind = (kind: Request["kind"]) => {
    setRequestKind(kind);
    const first = workspace.requests.find((r) => r.kind === kind);
    setSelected(first?.id || "");
    setActiveFolderId(first?.folderId || null);
    setSearch("");
    setHelp(false);
  };
  const addFolder = () => {
    const id = uid();
    const next = {
      ...workspace,
      requestFolders: [
        ...workspace.requestFolders,
        { id, name: "新目录", kind: requestKind, parentId: null },
      ],
    };
    try {
      mutate(moveFolder(next, id, activeFolderId));
      setSelected("");
      setActiveFolderId(id);
      setSearch("");
      setError("");
    } catch (e) {
      setError(String(e));
    }
  };
  const save = async () => {
    if (Object.keys(errors.current).length)
      throw Error("请先修正 JSON 编辑器中的格式错误");
    if (!native) throw Error("请通过桌面应用保存工作区");
    await invoke("save_workspace", { workspace });
    setDirty(false);
    setMessage("已保存到本地工作区");
  };
  const action = async (fn: () => Promise<unknown>) => {
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
  };
  const run = async () => {
    if (!current) return;
    setBusy(true);
    await action(async () => {
      await save();
      const kind =
        section === "requests"
          ? "request"
          : section === "collections"
            ? "collection"
            : "flow";
      const id = await invoke<string>("start_run", {
        kind,
        target: current,
        options: { ...options, environmentId },
      });
      setRunTarget(`${section}/${current.id}`);
      setSnapshot({ runId: id, status: "running", events: [] });
      setRunId(id);
    });
    setBusy(false);
  };
  const statuses: Record<string, string> = {};
  for (const e of runTarget === `${section}/${selected}`
    ? snapshot?.events || []
    : []) {
    if (e.nodeId) statuses[e.nodeId] = e.status;
  }
  const remove = () => {
    if (!current) return;
    mutate({ ...workspace, [section]: items.filter((i) => i.id !== selected) });
    setSelected(
      items.find(
        (i) =>
          i.id !== selected &&
          (section !== "requests" || i.kind === requestKind),
      )?.id || "",
    );
  };
  const runnable = ["requests", "collections", "flows"].includes(section);
  return (
    <ValidationContext.Provider value={report}>
      <div className="app-shell">
        <nav className="rail">
          <div className="brand" title="Boostman">
            b<span>›</span>
          </div>
          {(
            [
              "requests",
              "collections",
              "flows",
              "connections",
              "environments",
            ] as Section[]
          ).map((s, i) => (
            <button
              title={names[s]}
              key={s}
              className={section === s ? "active" : ""}
              onClick={() => choose(s)}
            >
              <span>{["↗", "▱", "⑂", "▤", "⚙"][i]}</span>
              <small>{names[s]}</small>
            </button>
          ))}
          <button
            className="help-button"
            onClick={() => setHelp(!help)}
            title="兼容范围"
          >
            <span>?</span>
            <small>指南</small>
          </button>
        </nav>
        <aside className="sidebar">
          <div className="workspace-label">
            <span className="workspace-avatar">B</span>
            <div>
              <strong>本地工作区</strong>
              <small>BOOSTMAN / PERSONAL</small>
            </div>
            <span className="local-dot" />
          </div>
          <div className="sidebar-title">
            <h2>{names[section]}</h2>
            <button
              className="icon"
              disabled={running}
              title="新建"
              onClick={() => add()}
            >
              ＋
            </button>
          </div>
          <div className="search">
            <span>⌕</span>
            <input
              placeholder="搜索名称…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          {section === "requests" ? (
            <RequestSidebar
              requests={workspace.requests}
              folders={workspace.requestFolders}
              kind={requestKind}
              onKind={switchKind}
              selected={selected}
              folderId={activeFolderId}
              query={search}
              disabled={running || !loaded}
              onAddRequest={() => add()}
              onAddFolder={addFolder}
              onRequest={(r) => {
                setSelected(r.id);
                setActiveFolderId(r.folderId || null);
              }}
              onFolder={(id) => {
                setActiveFolderId(id);
                setSelected("");
                setHelp(false);
              }}
            />
          ) : (
            <div className="sidebar-items">
              {items
                .filter((i) =>
                  i.name.toLowerCase().includes(search.toLowerCase()),
                )
                .map((i) => (
                  <button
                    className={`sidebar-item ${selected === i.id ? "selected" : ""}`}
                    key={i.id}
                    onClick={() => setSelected(i.id)}
                  >
                    <span
                      className={`method ${i.kind === "sql" ? "sql" : (i.method || "").toLowerCase()}`}
                    >
                      {section === "flows"
                        ? "⑂"
                        : section === "connections"
                          ? "DB"
                          : "▱"}
                    </span>
                    <span>{i.name}</span>
                  </button>
                ))}
              {items.length === 0 && (
                <div className="sidebar-empty">
                  还没有{names[section]}
                  <br />
                  点击 ＋ 创建第一个
                </div>
              )}
            </div>
          )}
          <div className="sidebar-bottom">
            <span className="local-dot" /> 本地执行 · Rust 核心
          </div>
        </aside>
        <main className="main">
          <header className="topbar">
            <div className="breadcrumb">
              工作区 <span>/</span> {names[section]}
            </div>
            <div className="top-actions">
              <select
                aria-label="运行环境"
                value={environmentId}
                disabled={running}
                onChange={(e) => setEnvironmentId(e.target.value)}
              >
                <option value="">无环境</option>
                {workspace.environments.map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.name}
                  </option>
                ))}
              </select>
              <button
                disabled={!loaded || running}
                onClick={() => void action(save)}
              >
                {dirty ? "● 保存更改" : "保存"}
              </button>
            </div>
          </header>
          {runnable && current && (
            <details className="execution-options">
              <summary>运行设置</summary>
              <div className="two-cols">
                <Field
                  label="脚本超时（毫秒）"
                  type="number"
                  value={options.scriptTimeoutMs}
                  onChange={(v) =>
                    setOptions({ ...options, scriptTimeoutMs: Number(v) })
                  }
                />
                {section !== "collections" && (
                  <label className="field">
                    <span>集合上下文（变量与继承脚本）</span>
                    <select
                      disabled={running}
                      value={options.collectionId || ""}
                      onChange={(e) =>
                        setOptions({ ...options, collectionId: e.target.value })
                      }
                    >
                      <option value="">不使用集合上下文</option>
                      {workspace.collections.map((c) => (
                        <option key={c.id} value={c.id}>
                          {c.name}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
              </div>
            </details>
          )}
          {(error || message) && (
            <div
              role="status"
              className={error ? "notification error-banner" : "notification"}
            >
              <span>{error || message}</span>
              <button
                className="icon"
                onClick={() => {
                  setError("");
                  setMessage("");
                }}
              >
                ×
              </button>
            </div>
          )}
          {help ? (
            <Guide onClose={() => setHelp(false)} />
          ) : section === "requests" && !current && activeFolder ? (
            <RequestFolderEditor
              folder={activeFolder}
              folders={workspace.requestFolders}
              requests={workspace.requests}
              disabled={running}
              onRename={(name) =>
                mutate({
                  ...workspace,
                  requestFolders: workspace.requestFolders.map((f) =>
                    f.id === activeFolder.id ? { ...f, name } : f,
                  ),
                })
              }
              onMove={(id) =>
                organize(() => moveFolder(workspace, activeFolder.id, id))
              }
              onDelete={() => {
                organize(() => removeFolder(workspace, activeFolder.id));
                setActiveFolderId(activeFolder.parentId);
                setSelected("");
              }}
              onAddRequest={() => add()}
              onAddFolder={addFolder}
            />
          ) : current ? (
            <>
              <div className="document-heading">
                <div className="doc-title">
                  <span className="doc-icon">
                    {section === "flows"
                      ? "⑂"
                      : section === "connections"
                        ? "▤"
                        : "↗"}
                  </span>
                  <input
                    aria-label="名称"
                    disabled={running}
                    value={current.name}
                    onChange={(e) =>
                      update({ ...current, name: e.target.value })
                    }
                  />
                  {dirty && <span className="dirty-dot" />}
                </div>
                <div className="document-actions">
                  {section === "requests" && (
                    <span className="badge">
                      {current.kind === "sql" ? "SQL REQUEST" : "HTTP REQUEST"}
                    </span>
                  )}
                  <button
                    className="danger"
                    title="删除当前项"
                    disabled={running}
                    onClick={remove}
                  >
                    删除当前项
                  </button>
                  {runnable &&
                    (running ? (
                      <button
                        className="stop"
                        disabled={busy}
                        onClick={() =>
                          void action(() => invoke("cancel_run", { runId }))
                        }
                      >
                        ■ 停止
                      </button>
                    ) : (
                      <button
                        className="primary"
                        disabled={!native || !loaded}
                        onClick={() => void run()}
                      >
                        {section === "requests" ? "发送请求" : "运行"}{" "}
                        <span>▶</span>
                      </button>
                    ))}
                </div>
              </div>
              {section === "requests" && (
                <div className="request-location">
                  <label>
                    所属目录{" "}
                    <select
                      aria-label="请求所属目录"
                      disabled={running}
                      value={current.folderId || ""}
                      onChange={(e) => {
                        const folderId = e.target.value || null;
                        organize(() =>
                          moveRequest(workspace, current.id, folderId),
                        );
                        setActiveFolderId(folderId);
                      }}
                    >
                      <option value="">根目录</option>
                      {folderChoices(
                        workspace.requestFolders,
                        current.kind,
                      ).map((f) => (
                        <option key={f.id} value={f.id}>
                          {folderPath(workspace.requestFolders, f.id)}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              )}
              <div className={`document-body ${running ? "locked" : ""}`}>
                <fieldset disabled={running} className="editor-fieldset">
                  {section === "requests" && (
                    <div className="editor-pad">
                      <RequestEditor
                        key={current.id}
                        compact
                        request={current as Request}
                        onChange={update}
                        connections={workspace.connections}
                      />
                    </div>
                  )}
                  {section === "collections" && (
                    <CollectionEditor
                      key={current.id}
                      collection={current as Collection}
                      requestFolders={workspace.requestFolders}
                      onChange={update}
                      requests={workspace.requests}
                      options={options}
                      onOptions={setOptions}
                    />
                  )}
                  {section === "flows" && (
                    <FlowEditor
                      key={current.id}
                      flow={current as Flow}
                      requestFolders={workspace.requestFolders}
                      onChange={update}
                      requests={workspace.requests}
                      connections={workspace.connections}
                      statuses={statuses}
                    />
                  )}
                  {section === "connections" && (
                    <ConnectionEditor
                      connection={current as DatabaseConnection}
                      onChange={update}
                      disabled={!native || busy}
                      onTest={() => {
                        setBusy(true);
                        void action(async () => {
                          await invoke("test_connection", {
                            connection: current,
                          });
                          setMessage("数据库连接成功");
                        }).finally(() => setBusy(false));
                      }}
                    />
                  )}
                  {section === "environments" && (
                    <div className="editor-pad settings-page">
                      <div className="section-title">
                        <div>
                          <h2>环境变量</h2>
                          <p>
                            用 {"{{变量名}}"} 在请求中引用，使用 FQL
                            在流程中读取。
                          </p>
                        </div>
                      </div>
                      <JsonEditor
                        key={current.id}
                        label="变量（JSON 对象）"
                        value={current.variables}
                        onChange={(variables) =>
                          update({ ...current, variables })
                        }
                      />
                      <div className="credential-card">
                        <h3>系统凭据库</h3>
                        <p>
                          为数据库密码或 Basic / Bearer
                          认证保存秘密，然后在配置中填写相同的凭据 ID。
                        </p>
                        <Field
                          label="凭据 ID"
                          value={secretId}
                          onChange={setSecretId}
                        />
                        <Field
                          label="秘密值"
                          type="password"
                          value={secretValue}
                          onChange={setSecretValue}
                        />
                        <div className="inline-add">
                          <button
                            disabled={!native || !secretId}
                            onClick={() =>
                              void action(async () => {
                                await invoke("set_secret", {
                                  id: secretId,
                                  secret: secretValue,
                                });
                                setSecretValue("");
                                setMessage("秘密已保存到系统凭据库");
                              })
                            }
                          >
                            保存秘密
                          </button>
                          <button
                            className="danger"
                            disabled={!native || !secretId}
                            onClick={() =>
                              void action(async () => {
                                await invoke("delete_secret", { id: secretId });
                                setMessage("凭据已删除");
                              })
                            }
                          >
                            删除凭据
                          </button>
                        </div>
                      </div>
                    </div>
                  )}
                </fieldset>
              </div>
              {runnable && (
                <ResponsePanel
                  key={runId}
                  snapshot={
                    runTarget === `${section}/${selected}` ? snapshot : null
                  }
                />
              )}
            </>
          ) : (
            <div className="welcome">
              <div className="welcome-kicker">BUILD · SEND · CONNECT</div>
              <h1>
                让每一步调试
                <br />
                连接成<span>完整流程。</span>
              </h1>
              <p>
                从一次 HTTP 请求开始，连接数据库、验证结果，
                <br />
                再把重复的工作交给 Flows。
              </p>
              <div className="welcome-actions">
                <button
                  className="primary"
                  disabled={!loaded}
                  onClick={() => add()}
                >
                  ＋ 新建
                  {section === "requests"
                    ? requestKind === "http"
                      ? " HTTP 请求"
                      : " SQL 请求"
                    : names[section]}
                </button>
                {section === "requests" && (
                  <button disabled={!loaded || running} onClick={addFolder}>
                    新建目录
                  </button>
                )}
              </div>
              <div className="feature-grid">
                <div>
                  <b>01</b>
                  <h3>调试请求</h3>
                  <p>
                    HTTP(S) 与 SQL，
                    <br />
                    共用一套执行核心。
                  </p>
                </div>
                <div>
                  <b>02</b>
                  <h3>批量验证</h3>
                  <p>
                    组织集合，驱动数据，
                    <br />
                    让断言覆盖每一次执行。
                  </p>
                </div>
                <div>
                  <b>03</b>
                  <h3>编排流程</h3>
                  <p>
                    条件、循环与 FQL，
                    <br />
                    把数据交给下一步。
                  </p>
                </div>
              </div>
            </div>
          )}
          <footer className="statusbar">
            <span>
              <i className="local-dot" /> {native ? "桌面执行引擎" : "界面预览"}{" "}
              <span className="divider">|</span> 工作区 v1
            </span>
            <span>
              {running ? "正在运行…" : dirty ? "有未保存的更改" : "就绪"}{" "}
              <span className="divider">|</span> Boostman 0.1
            </span>
          </footer>
        </main>
      </div>
    </ValidationContext.Provider>
  );
}
function Guide({ onClose }: { onClose: () => void }) {
  return (
    <div className="help-page editor-pad">
      <h1>执行与兼容指南</h1>
      <p>
        HTTP、SQL、集合及流程均在 Rust 中执行。工作区保存在应用数据目录的 SQLite
        数据库中；独立配置的密码与 Token 使用系统凭据库。
      </p>
      <h3>FQL 常用子集</h3>
      <p>
        支持字段访问、索引、过滤、对象/数组构造、比较、布尔逻辑、算术、三元条件及
        fn 回调。
      </p>
      <pre>
        {
          "$map(input.rows, fn($row){ $row.id })\ninput.rows[enabled = true]\nvariables.token"
        }
      </pre>
      <p>
        $map / $filter / $count / $sum / $exists / $keys / $lookup / $join /
        $split / $uppercase / $lowercase / $trim / $json / $jsonParse
      </p>
      <p>不支持的语法或函数会明确报错。这是 Postman FQL 的常用兼容子集。</p>
      <h3>Postman 脚本子集</h3>
      <p>
        支持集合 → 父文件夹 → 请求的前置和后置脚本。提供
        pm.variables、environment、collectionVariables、iterationData，pm.request
        的 URL/Headers/Raw Body 修改，pm.response 的
        code/headers/json()/text()，pm.test、常用 pm.expect 及回调/Promise
        形式的 pm.sendRequest。
      </p>
      <p>
        断言支持 equal / eql / deep.equal / include / property / a / an /
        lengthOf / not，以及 response.to.have.status。不支持 Node.js
        API、外部包、执行跳转、OAuth 授权流程及高级连接配置。
      </p>
      <h3>运行规则</h3>
      <p>
        变量优先级：局部 → 迭代数据 → 环境 →
        集合。集合/环境脚本写入会保存。Cookie 在同一次运行中共享；HTTP 4xx/5xx
        需用断言判断失败。SQL 的 NULL 参数如需指定类型，请使用{" "}
        {'{"type":"i64","value":null}'}。
      </p>
      <p>
        循环默认最多 1000
        次，允许嵌套；只执行当前分支。取消会停止后续执行，已经提交的 SQL
        写入不会回滚。运行记录仅保留在本次应用会话中。
      </p>
      <button onClick={onClose}>返回工作区</button>
    </div>
  );
}
