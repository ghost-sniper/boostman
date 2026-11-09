import {
  createContext,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import type { Data, Request } from "./model";
export const ValidationContext = createContext<
  (id: string, error: string) => void
>(() => {});
export function JsonEditor({
  value,
  onChange,
  label,
  describedBy,
  rows = 5,
}: {
  value: any;
  onChange: (v: any) => void;
  label?: string;
  describedBy?: string;
  rows?: number;
}) {
  const id = useId(),
    report = useContext(ValidationContext);
  const [text, setText] = useState(
    JSON.stringify(value === undefined ? {} : value, null, 2),
  );
  const [error, setError] = useState("");
  const serialized = JSON.stringify(value === undefined ? {} : value);
  const emitted = useRef(serialized);
  useEffect(() => {
    if (serialized !== emitted.current) {
      setText(JSON.stringify(value === undefined ? {} : value, null, 2));
      setError("");
      report(id, "");
      emitted.current = serialized;
    }
  }, [serialized]);
  useEffect(() => () => report(id, ""), [id, report]);
  return (
    <label className="field json-field">
      {label && <span>{label}</span>}
      <textarea
        aria-describedby={describedBy}
        spellCheck={false}
        rows={rows}
        className={error ? "invalid code" : "code"}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          try {
            const v = JSON.parse(e.target.value);
            emitted.current = JSON.stringify(v);
            setError("");
            report(id, "");
            onChange(v);
          } catch {
            setError("JSON 格式不正确");
            report(id, "JSON 格式不正确");
          }
        }}
      />
      {error && <small className="error">{error}</small>}
    </label>
  );
}
export function Field({
  label,
  value,
  onChange,
  type = "text",
}: {
  label: string;
  value: any;
  onChange: (s: string) => void;
  type?: string;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        type={type}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
export function Pairs({
  value,
  onChange,
  files = false,
}: {
  value: Data[];
  onChange: (v: Data[]) => void;
  files?: boolean;
}) {
  const update = (i: number, p: Data) =>
    onChange(value.map((r, j) => (i === j ? { ...r, ...p } : r)));
  return (
    <div className="pairs">
      <div className="pair-head">
        启用 / 参数名 <span>值 {files && "（文件项填写本地绝对路径）"}</span>
      </div>
      {value.map((r, i) => (
        <div className="pair-row" key={i}>
          <input
            type="checkbox"
            checked={r.enabled !== false}
            onChange={(e) => update(i, { enabled: e.target.checked })}
          />
          <input
            placeholder="Key"
            value={r.key}
            onChange={(e) => update(i, { key: e.target.value })}
          />
          <input
            placeholder="Value · 支持 {{变量}}"
            value={r.value}
            onChange={(e) => update(i, { value: e.target.value })}
          />
          {files && (
            <select
              value={r.type || "text"}
              onChange={(e) => update(i, { type: e.target.value })}
            >
              <option value="text">文本</option>
              <option value="file">文件</option>
            </select>
          )}
          <button
            className="icon"
            onClick={() => onChange(value.filter((_, j) => j !== i))}
          >
            ×
          </button>
        </div>
      ))}
      <button
        className="subtle"
        onClick={() =>
          onChange([
            ...value,
            { key: "", value: "", enabled: true, type: "text" },
          ])
        }
      >
        ＋ 添加参数
      </button>
    </div>
  );
}
const sqlParameterExamples = [
  ["string", "字符串", "active"],
  ["i64", "有符号整数", "1"],
  ["u64", "无符号整数", "1"],
  ["f64", "浮点数", 1.25],
  ["bool", "布尔值", true],
  ["decimal", "精确小数", "1.20"],
  ["json", "JSON 对象或数组", { enabled: true }],
  ["bytes", "Base64 二进制", "AP8="],
] as const;

export function RequestEditor({
  request: r,
  onChange,
  connections,
  compact = false,
}: {
  request: Request;
  onChange: (v: Request) => void;
  connections: Data[];
  compact?: boolean;
}) {
  const [tab, setTab] = useState("params");
  const parameterHint = useId();
  const set = (p: Data) => onChange({ ...r, ...p });
  const body = r.body || { type: "none" };
  const auth = r.auth || { type: "none" };
  return (
    <div className="request-editor">
      {!compact && (
        <Field
          label="请求名称"
          value={r.name}
          onChange={(name) => set({ name })}
        />
      )}
      {r.kind === "sql" ? (
        <>
          <div className="two-cols">
            <label className="field">
              <span>数据库连接</span>
              <select
                value={r.connectionId || ""}
                onChange={(e) => {
                  const oracle =
                    connections.find((c) => c.id === e.target.value)?.driver ===
                    "oracle";
                  set({
                    connectionId: e.target.value,
                    sql:
                      oracle && r.sql === "SELECT 1 AS connected"
                        ? "SELECT 1 AS connected FROM dual"
                        : !oracle && r.sql === "SELECT 1 AS connected FROM dual"
                          ? "SELECT 1 AS connected"
                          : r.sql,
                  });
                }}
              >
                <option value="">选择连接</option>
                {connections.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name} · {c.driver}
                  </option>
                ))}
              </select>
            </label>
            <Field
              label="超时（毫秒）"
              value={r.timeoutMs}
              type="number"
              onChange={(v) => set({ timeoutMs: Number(v) })}
            />
          </div>
          <label className="field">
            <span>SQL · 通用参数 $1、$2，也支持 MySQL ? 和 Oracle :1、:2</span>
            <textarea
              rows={7}
              className="code"
              value={r.sql || ""}
              onChange={(e) => set({ sql: e.target.value })}
            />
          </label>
          <div className="sql-parameters">
            <JsonEditor
              label="绑定参数（JSON 数组）"
              describedBy={parameterHint}
              value={r.params || []}
              onChange={(params) => set({ params })}
            />
            <p id={parameterHint} className="muted">
              <code>$1</code> 对应数组第一个值。可直接填写数字、字符串、布尔值或
              null，例如 <code>[1, "active", true, null]</code>。
              {connections.find((c) => c.id === r.connectionId)?.driver ===
                "postgres" && (
                <>
                  {" "}
                  PostgreSQL 按 SQL 推断的类型绑定。<code>
                    INT4
                  </code> 列可直接传 <code>[1]</code>。
                </>
              )}
            </p>
            <details className="sql-parameter-help">
              <summary>参数类型与 JSON 示例</summary>
              <p>
                以下示例是数组中的单个值。大整数和精确小数的 <code>value</code>{" "}
                用字符串填写，以保留精度。
              </p>
              <ul>
                {sqlParameterExamples.map(([type, name, value]) => (
                  <li key={type}>
                    {name} <code>{JSON.stringify({ type, value })}</code>
                  </li>
                ))}
                <li>
                  带类型的空值 <code>{'{"type":"i64","value":null}'}</code>
                </li>
              </ul>
            </details>
          </div>
        </>
      ) : (
        <>
          <div className="url-bar">
            <select
              value={r.method}
              onChange={(e) => set({ method: e.target.value })}
            >
              {![
                "GET",
                "POST",
                "PUT",
                "PATCH",
                "DELETE",
                "HEAD",
                "OPTIONS",
              ].includes(r.method) && (
                <option value={r.method}>{r.method}</option>
              )}
              {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map(
                (m) => (
                  <option key={m}>{m}</option>
                ),
              )}
            </select>
            <input
              aria-label="URL"
              placeholder="https://api.example.com/{{path}}"
              value={r.url}
              onChange={(e) => set({ url: e.target.value })}
            />
          </div>
          <div className="tabs">
            {[
              ["params", "参数"],
              ["headers", "Headers"],
              ["body", "请求体"],
              ["auth", "认证"],
              ["settings", "设置"],
            ].map(([id, title]) => (
              <button
                key={id}
                className={tab === id ? "active" : ""}
                onClick={() => setTab(id)}
              >
                {title}
                {id === "headers" && r.headers?.length > 0
                  ? ` · ${r.headers.length}`
                  : ""}
              </button>
            ))}
          </div>
          {tab === "params" && (
            <Pairs value={r.query || []} onChange={(query) => set({ query })} />
          )}
          {tab === "headers" && (
            <Pairs
              value={r.headers || []}
              onChange={(headers) => set({ headers })}
            />
          )}
          {tab === "body" && (
            <>
              <select
                value={body.type}
                onChange={(e) =>
                  set({ body: { ...body, type: e.target.value } })
                }
              >
                {[
                  ["none", "无"],
                  ["json", "JSON"],
                  ["text", "文本"],
                  ["form", "URL 编码表单"],
                  ["multipart", "Multipart"],
                  ["file", "二进制文件"],
                ].map(([v, n]) => (
                  <option value={v} key={v}>
                    {n}
                  </option>
                ))}
              </select>
              {["json", "text"].includes(body.type) && (
                <textarea
                  className="code body-editor"
                  rows={8}
                  value={body.text || ""}
                  onChange={(e) =>
                    set({ body: { ...body, text: e.target.value } })
                  }
                />
              )}
              {["form", "multipart"].includes(body.type) && (
                <Pairs
                  files={body.type === "multipart"}
                  value={body.fields || []}
                  onChange={(fields) => set({ body: { ...body, fields } })}
                />
              )}
              {body.type === "file" && (
                <Field
                  label="文件绝对路径"
                  value={body.path}
                  onChange={(path) => set({ body: { ...body, path } })}
                />
              )}
            </>
          )}
          {tab === "auth" && (
            <div className="two-cols">
              <label className="field">
                <span>认证类型</span>
                <select
                  value={auth.type}
                  onChange={(e) =>
                    set({ auth: { ...auth, type: e.target.value } })
                  }
                >
                  <option value="none">无认证</option>
                  <option value="basic">Basic Auth</option>
                  <option value="bearer">Bearer Token</option>
                </select>
              </label>
              {auth.type === "basic" && (
                <Field
                  label="用户名"
                  value={auth.username}
                  onChange={(username) => set({ auth: { ...auth, username } })}
                />
              )}
              {auth.type !== "none" && (
                <Field
                  label="凭据 ID（在环境与凭据中设置秘密）"
                  value={auth.secretId}
                  onChange={(secretId) => set({ auth: { ...auth, secretId } })}
                />
              )}
            </div>
          )}
          {tab === "settings" && (
            <div className="two-cols">
              <Field
                label="超时（毫秒）"
                type="number"
                value={r.timeoutMs}
                onChange={(v) => set({ timeoutMs: Number(v) })}
              />
              <label className="check">
                <input
                  type="checkbox"
                  checked={r.followRedirects !== false}
                  onChange={(e) => set({ followRedirects: e.target.checked })}
                />{" "}
                跟随重定向（最多 10 次）
              </label>
            </div>
          )}
        </>
      )}
      <details className="scripts">
        <summary>
          脚本与输入映射 <span>JavaScript / FQL</span>
        </summary>
        <div className="two-cols">
          <label className="field">
            <span>前置脚本</span>
            <textarea
              rows={7}
              className="code"
              placeholder="pm.variables.set('token', '...');"
              value={r.preScript || ""}
              onChange={(e) => set({ preScript: e.target.value })}
            />
          </label>
          <label className="field">
            <span>后置脚本 / 测试</span>
            <textarea
              rows={7}
              className="code"
              placeholder="pm.test('状态正常', () => { pm.expect(pm.response.code).to.equal(200); });"
              value={r.postScript || ""}
              onChange={(e) => set({ postScript: e.target.value })}
            />
          </label>
        </div>
        <JsonEditor
          label="FQL 输入映射（字段名 → 表达式，SQL 仅允许 params）"
          value={r.bindings || {}}
          onChange={(bindings) => set({ bindings })}
        />
      </details>
    </div>
  );
}
