import { useEffect, useRef, useState } from "react";
import { consoleEntries } from "./console";
import type { Data } from "./model";
export function ConsolePanel({ snapshot }: { snapshot: Data }) {
  const [level, setLevel] = useState("all"),
    [follow, setFollow] = useState(true);
  const end = useRef<HTMLDivElement>(null);
  const entries = consoleEntries(snapshot),
    visible = entries.filter((e) => level === "all" || e.level === level);
  useEffect(() => {
    if (follow) end.current?.scrollIntoView({ block: "nearest" });
  }, [entries.length, entries[entries.length - 1]?.logId, follow]);
  return (
    <div className="run-console">
      <div className="console-toolbar">
        <span>整个运行 · {entries.length} 条输出</span>
        <select
          aria-label="日志级别"
          value={level}
          onChange={(e) => setLevel(e.target.value)}
        >
          <option value="all">全部级别</option>
          {["log", "info", "warn", "error", "debug"].map((v) => (
            <option key={v}>{v}</option>
          ))}
        </select>
        <label>
          <input
            type="checkbox"
            checked={follow}
            onChange={(e) => setFollow(e.target.checked)}
          />{" "}
          跟随输出
        </label>
      </div>
      <div role="log" aria-live="polite" className="console-lines">
        {visible.length ? (
          visible.map((e) => (
            <div key={e.logId} className={`console-line console-${e.level}`}>
              <span className="console-level">{e.level}</span>
              <div>
                <small>
                  {e.timestamp && new Date(e.timestamp).toLocaleTimeString()} ·{" "}
                  {e.nodeId || "请求"} ·{" "}
                  {e.phase === "pre"
                    ? "前置脚本"
                    : e.phase === "post"
                      ? "后置脚本"
                      : e.phase === "flow"
                        ? "Log 组件"
                        : ""}{" "}
                  · 迭代 {JSON.stringify(e.path)}
                </small>
                <pre>{e.message}</pre>
              </div>
            </div>
          ))
        ) : (
          <p className="muted">
            在请求或 SQL 的前置/后置脚本中调用
            console.log()，输出会实时显示在这里。
          </p>
        )}
        <div ref={end} />
      </div>
    </div>
  );
}
