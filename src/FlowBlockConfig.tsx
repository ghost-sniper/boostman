import { Field, JsonEditor } from "./components";
import { displayModes } from "./flowPresentation";
import type { Data } from "./model";
import { uid } from "./model";
export function FlowBlockConfig({
  data: d,
  onChange: set,
}: {
  data: Data;
  onChange: (data: Data) => void;
}) {
  const entries: Data[] = d.entries || [];
  const update = (i: number, patch: Data) =>
    set({ entries: entries.map((v, j) => (j === i ? { ...v, ...patch } : v)) });
  return (
    <>
      {d.kind === "constant" && (
        <JsonEditor
          label="常量值（任意 JSON，包括 null）"
          value={d.value}
          onChange={(value) => set({ value })}
        />
      )}
      {d.kind === "string" && (
        <label className="field">
          <span>字符串内容</span>
          <textarea
            rows={4}
            value={d.value ?? ""}
            onChange={(e) => set({ value: e.target.value })}
          />
        </label>
      )}
      {d.kind === "number" && (
        <JsonEditor
          label="数值（JSON number）"
          value={d.value}
          onChange={(value) => set({ value })}
        />
      )}
      {d.kind === "bool" && (
        <label className="field">
          <span>布尔值</span>
          <select
            value={String(d.value)}
            onChange={(e) => set({ value: e.target.value === "true" })}
          >
            <option value="true">true</option>
            <option value="false">false</option>
          </select>
        </label>
      )}
      {d.kind === "null" && <p className="muted">输出 JSON null。</p>}
      {d.kind === "now" && (
        <p className="muted">
          由 Rust 在执行时获取当前时间，输出 Unix 毫秒时间戳。
        </p>
      )}
      {d.kind === "date" && (
        <>
          <Field
            label="日期（UTC 零点）"
            type="date"
            value={d.value}
            onChange={(value) => set({ value })}
          />
          <p className="muted">输出所选日期 UTC 零点的 Unix 毫秒时间戳。</p>
        </>
      )}
      {d.kind === "datetime" && (
        <>
          <Field
            label="日期时间（带时区的 ISO 8601）"
            value={d.value}
            onChange={(value) => set({ value })}
          />
          <p className="muted">
            例如 2026-11-06T12:30:00+08:00；输出 Unix 毫秒时间戳。
          </p>
        </>
      )}
      {["createVariable", "getVariable"].includes(d.kind) && (
        <>
          <Field
            label="流程变量名称"
            value={d.variableName}
            onChange={(variableName) => set({ variableName })}
          />
          <p className="muted">
            变量仅在本次运行中保存；Get Variable 必须在对应 Create Variable
            执行之后使用，实际 null 也会保留。
          </p>
        </>
      )}
      {d.kind === "or" && (
        <p className="muted">
          {d.portMode ? (
            "两个输入端口中先到达的数据立即输出，不等待另一个端口；false 与 null 也会透传。"
          ) : (
            <>
              结构化分支合流：放在 If / Condition / Validate
              之后，将已执行分支的输出继续传递，包括 false 和
              null；当前顺序引擎不等待或竞争并行端口。
            </>
          )}
        </p>
      )}
      {d.kind === "collect" && (
        <p className="muted">
          {d.portMode ? (
            "连接循环体最后一个组件的输出到 Item；循环结束后 List 输出有序数组，Finish 输出完成事件。零次循环也会输出空数组。"
          ) : (
            <>
              放在循环体中会收集各轮表达式结果，循环结束后在本节点结果中输出完整数组；嵌套循环分别收集。也可放在循环之后接收循环结果数组。最多
              10000 项。
            </>
          )}
        </p>
      )}
      {d.portMode && (
        <details>
          <summary>输入变量</summary>
          {(d.inputBindings || []).map((b: Data, i: number) => (
            <div className="block-entry" key={b.id}>
              <Field
                label="变量名称"
                value={b.name}
                onChange={(name) =>
                  set({
                    inputBindings: d.inputBindings.map((v: Data) =>
                      v.id === b.id ? { ...v, name } : v,
                    ),
                  })
                }
              />
              <Field
                label="未连接时的 FQL 值"
                value={b.expression}
                onChange={(expression) =>
                  set({
                    inputBindings: d.inputBindings.map((v: Data) =>
                      v.id === b.id ? { ...v, expression } : v,
                    ),
                  })
                }
              />
              <button
                onClick={() =>
                  set({
                    inputBindings: d.inputBindings.filter(
                      (_: Data, j: number) => j !== i,
                    ),
                  })
                }
              >
                移除输入变量
              </button>
            </div>
          ))}
          <button
            onClick={() =>
              set({
                inputBindings: [
                  ...(d.inputBindings || []),
                  {
                    id: uid(),
                    name: `value${(d.inputBindings || []).length + 1}`,
                    expression: "null",
                  },
                ],
              })
            }
          >
            ＋ 输入变量
          </button>
        </details>
      )}
      {d.kind === "validate" && (
        <>
          <JsonEditor
            label="JSON Schema"
            value={d.schema}
            onChange={(schema) => set({ schema })}
          />
          <p className="muted">
            按 JSON Schema 校验 input，并进入 Pass 或 Fail
            子流程。类型不符会进入 Fail，Schema
            本身错误会中止。仅解析内嵌引用，不读取外部 URL 或文件。
          </p>
        </>
      )}
      {d.kind === "condition" && (
        <div className="block-entries">
          <p className="muted">
            按顺序检查 FQL 布尔表达式，只执行首个 true
            对应的子流程；均不成立则进入 Default。
          </p>
          {(d.conditions || []).map((c: Data, i: number) => (
            <div className="block-entry" key={c.id}>
              <Field
                label="分支名称"
                value={c.name}
                onChange={(name) =>
                  set({
                    conditions: d.conditions.map((v: Data, j: number) =>
                      j === i ? { ...v, name } : v,
                    ),
                  })
                }
              />
              <Field
                label="条件（FQL boolean）"
                value={c.expression}
                onChange={(expression) =>
                  set({
                    conditions: d.conditions.map((v: Data, j: number) =>
                      j === i ? { ...v, expression } : v,
                    ),
                  })
                }
              />
              <button
                disabled={i === 0}
                onClick={() => {
                  const next = [...d.conditions];
                  [next[i - 1], next[i]] = [next[i], next[i - 1]];
                  set({ conditions: next });
                }}
              >
                上移
              </button>
              <button
                className="danger"
                onClick={() =>
                  set({
                    conditions: d.conditions.filter(
                      (_: Data, j: number) => i !== j,
                    ),
                  })
                }
              >
                移除分支及其节点
              </button>
            </div>
          ))}
          <button
            onClick={() =>
              set({
                conditions: [
                  ...(d.conditions || []),
                  {
                    id: uid(),
                    name: `条件 ${(d.conditions || []).length + 1}`,
                    expression: "false",
                  },
                ],
              })
            }
          >
            ＋ 添加条件
          </button>
        </div>
      )}
      {d.kind === "response" && (
        <>
          <Field
            label="HTTP 状态码"
            type="number"
            value={d.statusCode}
            onChange={(v) => set({ statusCode: Number(v) })}
          />
          <JsonEditor
            label="响应 Headers（JSON 对象，值为字符串）"
            value={d.headers}
            onChange={(headers) => set({ headers })}
          />
          <p className="muted">
            生成本地响应结果 {"{statusCode, headers, body}"}
            ，写入运行快照；不启动 HTTP 服务或发布 URL。一条执行路径只能执行一个
            Response。
          </p>
        </>
      )}
      {d.kind === "output" && (
        <p className="muted">
          将表达式结果设为流程的最终输出，保存到运行快照
          output。每个流程只允许一个主流程 Output；端口画布可通过 Flow
          模块组件调用该流程并接收 Output。
        </p>
      )}
      {["list", "record"].includes(d.kind) && (
        <div className="block-entries">
          <p className="muted">
            每项使用 FQL。字符串用引号包围，可引用
            input、variables、nodes、item、index。各项读取同一输入上下文。
          </p>
          {entries.map((e, i) => (
            <div className="block-entry" key={i}>
              {d.kind === "record" && (
                <Field
                  label={`字段名 ${i + 1}`}
                  value={e.key}
                  onChange={(key) => update(i, { key })}
                />
              )}
              <Field
                label={`值表达式 ${i + 1}`}
                value={e.expression}
                onChange={(expression) => update(i, { expression })}
              />
              <div>
                <button
                  disabled={i === 0}
                  onClick={() => {
                    const next = [...entries];
                    [next[i - 1], next[i]] = [next[i], next[i - 1]];
                    set({ entries: next });
                  }}
                >
                  上移
                </button>
                <button
                  className="danger"
                  onClick={() =>
                    set({ entries: entries.filter((_, j) => j !== i) })
                  }
                >
                  移除
                </button>
              </div>
            </div>
          ))}
          <button
            onClick={() =>
              set({
                entries: [
                  ...entries,
                  { key: `field${entries.length + 1}`, expression: "null" },
                ],
              })
            }
          >
            ＋ 添加{d.kind === "list" ? "项" : "字段"}
          </button>
        </div>
      )}
      {d.kind === "select" && (
        <>
          <Field
            label="JSON Pointer 路径"
            value={d.path}
            onChange={(path) => set({ path })}
          />
          <p className="muted">
            例如 /rows/0/id；空路径返回整个输入。字段中的 / 写为 ~1，~ 写为
            ~0。路径不存在时明确报错。
          </p>
        </>
      )}
      {[
        "display",
        "log",
        "delay",
        "createVariable",
        "collect",
        "output",
        "response",
      ].includes(d.kind) && (
        <>
          <label className="field">
            <span>
              {d.kind === "delay"
                ? "延迟毫秒数（FQL，0–600000 整数）"
                : "数据表达式（FQL）"}
            </span>
            <textarea
              className="code"
              rows={3}
              value={d.expression || ""}
              onChange={(e) => set({ expression: e.target.value })}
            />
          </label>
          {["display", "log", "delay"].includes(d.kind) && (
            <p className="muted">此组件会将原始 input 继续传给下一步。</p>
          )}
        </>
      )}
      {d.kind === "display" && (
        <>
          <label className="field">
            <span>展示格式</span>
            <select
              value={d.displayMode || "json"}
              onChange={(e) => set({ displayMode: e.target.value })}
            >
              {Object.entries(displayModes).map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </select>
          </label>
          {d.displayMode === "line" && (
            <>
              <Field
                label="横轴字段"
                value={d.xKey || "x"}
                onChange={(xKey) => set({ xKey })}
              />
              <Field
                label="纵轴字段"
                value={d.yKey || "y"}
                onChange={(yKey) => set({ yKey })}
              />
              <p className="muted">
                输入数字数组，或包含横轴和数值纵轴字段的对象数组。
              </p>
            </>
          )}
        </>
      )}
      {d.kind === "log" && (
        <label className="field">
          <span>日志级别</span>
          <select
            value={d.level || "log"}
            onChange={(e) => set({ level: e.target.value })}
          >
            {["log", "info", "warn", "error", "debug"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        </label>
      )}
    </>
  );
}
