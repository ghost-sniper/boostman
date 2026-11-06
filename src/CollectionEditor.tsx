import { useState } from "react";
import { Field, JsonEditor } from "./components";
import { uid } from "./model";
import type { Collection, Data, Request } from "./model";
import type { RequestFolder } from "./model";
import { requestLabel } from "./requestFolders";
function Items({
  items,
  onChange,
  requests,
  requestFolders,
  depth = 0,
}: {
  items: Data[];
  onChange: (v: Data[]) => void;
  requests: Request[];
  requestFolders: RequestFolder[];
  depth?: number;
}) {
  const [pick, setPick] = useState(requests[0]?.id || "");
  const update = (i: number, v: Data) =>
    onChange(items.map((a, j) => (i === j ? v : a)));
  const move = (i: number, delta: number) => {
    const next = [...items];
    const target = i + delta;
    if (target < 0 || target >= next.length) return;
    [next[i], next[target]] = [next[target], next[i]];
    onChange(next);
  };
  return (
    <div className="collection-items">
      {items.map((item, i) => (
        <div className="collection-item" key={item.id}>
          <div className="item-bar">
            <span className="muted">{i + 1}.</span>
            <strong>
              {item.type === "folder"
                ? "▱ " + item.name
                : requests.find((r) => r.id === item.requestId)?.name ||
                  "请求已删除"}
            </strong>
            <button
              className="icon"
              title="上移"
              disabled={i === 0}
              onClick={() => move(i, -1)}
            >
              ↑
            </button>
            <button
              className="icon"
              title="下移"
              disabled={i === items.length - 1}
              onClick={() => move(i, 1)}
            >
              ↓
            </button>
            <button
              className="icon danger"
              title="删除"
              onClick={() => onChange(items.filter((_, j) => j !== i))}
            >
              ×
            </button>
          </div>
          {item.type === "folder" && (
            <details open>
              <summary>文件夹配置</summary>
              <Field
                label="文件夹名称"
                value={item.name}
                onChange={(name) => update(i, { ...item, name })}
              />
              <div className="two-cols">
                {["preScript", "postScript"].map((key, j) => (
                  <label key={key} className="field">
                    <span>{j === 0 ? "前置脚本" : "后置脚本"}</span>
                    <textarea
                      rows={3}
                      className="code"
                      value={item[key] || ""}
                      onChange={(e) =>
                        update(i, { ...item, [key]: e.target.value })
                      }
                    />
                  </label>
                ))}
              </div>
              <Items
                depth={depth + 1}
                items={item.items || []}
                requests={requests}
                requestFolders={requestFolders}
                onChange={(children) => update(i, { ...item, items: children })}
              />
            </details>
          )}
        </div>
      ))}
      <div className="inline-add">
        <select value={pick} onChange={(e) => setPick(e.target.value)}>
          <option value="">选择请求</option>
          {requests.map((r) => (
            <option key={r.id} value={r.id}>
              {r.kind === "sql" ? "SQL" : r.method} ·{" "}
              {requestLabel(r, requestFolders)}
            </option>
          ))}
        </select>
        <button
          disabled={!pick}
          onClick={() =>
            onChange([
              ...items,
              { id: uid(), type: "request", requestId: pick },
            ])
          }
        >
          ＋ 添加请求
        </button>
        <button
          disabled={depth >= 16}
          onClick={() =>
            onChange([
              ...items,
              {
                id: uid(),
                type: "folder",
                name: "新文件夹",
                items: [],
                preScript: "",
                postScript: "",
              },
            ])
          }
        >
          ＋ 文件夹
        </button>
      </div>
    </div>
  );
}
export function CollectionEditor({
  collection: c,
  onChange,
  requests,
  requestFolders = [],
  options,
  onOptions,
}: {
  collection: Collection;
  onChange: (v: Collection) => void;
  requests: Request[];
  requestFolders?: RequestFolder[];
  options: Data;
  onOptions: (v: Data) => void;
}) {
  return (
    <div className="editor-pad">
      <div className="section-title">
        <div>
          <h2>集合运行</h2>
          <p>有序执行请求，用数据迭代验证结果。</p>
        </div>
        <span className="badge">COLLECTION RUNNER</span>
      </div>
      <div className="runner-options">
        <Field
          label="迭代轮数"
          type="number"
          value={options.iterations || 1}
          onChange={(v) => onOptions({ ...options, iterations: Number(v) })}
        />
        <label className="check">
          <input
            type="checkbox"
            checked={!!options.continueOnError}
            onChange={(e) =>
              onOptions({ ...options, continueOnError: e.target.checked })
            }
          />{" "}
          失败后继续执行
        </label>
      </div>
      <details>
        <summary>迭代数据、变量和脚本</summary>
        <div className="two-cols">
          <JsonEditor
            label="迭代数据（JSON 对象数组）"
            value={options.data || [{}]}
            onChange={(data) => onOptions({ ...options, data })}
          />
          <JsonEditor
            label="集合变量"
            value={c.variables}
            onChange={(variables) => onChange({ ...c, variables })}
          />
        </div>
        <div className="two-cols">
          {["preScript", "postScript"].map((key, i) => (
            <label className="field" key={key}>
              <span>集合{i === 0 ? "前置" : "后置"}脚本</span>
              <textarea
                className="code"
                rows={5}
                value={c[key] || ""}
                onChange={(e) => onChange({ ...c, [key]: e.target.value })}
              />
            </label>
          ))}
        </div>
      </details>
      <h3>执行顺序</h3>
      <Items
        items={c.items}
        requests={requests}
        requestFolders={requestFolders}
        onChange={(items) => onChange({ ...c, items })}
      />
    </div>
  );
}
