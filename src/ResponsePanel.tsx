import { useId, useState } from "react";
import type { Data } from "./model";
import { ConsolePanel } from "./ConsolePanel";
import { consoleEntries } from "./console";
import { FlowDisplay } from "./FlowDisplay";
import { flowResults } from "./flowPresentation";
const stateLabels: Record<string, string> = {
  running: "运行中",
  succeeded: "成功",
  failed: "失败",
  cancelled: "已取消",
};
export function ResponsePanel({
  snapshot,
  collapsed,
  onToggle,
}: {
  snapshot: Data | null;
  collapsed: boolean;
  onToggle: () => void;
}) {
  const contentId = useId();
  const toggle = (
    <button
      className="response-toggle"
      aria-expanded={!collapsed}
      aria-controls={contentId}
      onClick={onToggle}
    >
      {collapsed ? "展开执行结果 ▴" : "收起执行结果 ▾"}
    </button>
  );
  const [tab, setTab] = useState("json"),
    [step, setStep] = useState(-1);
  if (!snapshot)
    return (
      <section className={`response-panel ${collapsed ? "is-collapsed" : ""}`}>
        <div className="panel-heading">
          <div>
            <strong>响应</strong>
            <span className="muted">等待运行</span>
          </div>
          {toggle}
        </div>
        <div id={contentId} className="response-content" hidden={collapsed}>
          <div className="response-empty">
            <div>↗</div>
            <h3>每一次请求，都有迹可循</h3>
            <p>发送请求或运行流程，在这里查看响应、断言和执行记录。</p>
          </div>
        </div>
      </section>
    );
  const events = (snapshot.events || []).filter(
    (e: Data) => e.type === "step" && e.result,
  );
  const final = snapshot.result;
  const selected = step >= 0 ? events[step] : events[events.length - 1];
  const result = selected?.result || final;
  const tests = result?.tests || [];
  return (
    <section className={`response-panel ${collapsed ? "is-collapsed" : ""}`}>
      <div className="panel-heading">
        <div>
          <strong>执行结果</strong>
          <span className={`status ${snapshot.status}`}>
            {stateLabels[snapshot.status] || snapshot.status}
          </span>
        </div>
        <div className="metrics">
          {result?.meta?.statusCode && <b>{result.meta.statusCode}</b>}
          <span>{result?.elapsedMs ?? final?.elapsedMs ?? "—"} ms</span>
          {result?.meta?.size !== undefined && (
            <span>{result.meta.size} B</span>
          )}
          <span>{events.length} 个步骤</span>
          {toggle}
        </div>
      </div>
      <div id={contentId} className="response-content" hidden={collapsed}>
        <div className="tabs">
          {[
            ["json", "JSON"],
            ["raw", "原始内容"],
            ["headers", "Headers"],
            ["tests", `断言 · ${tests.length}`],
            ["logs", `控制台 · ${consoleEntries(snapshot).length}`],
            ["timeline", "执行记录"],
            ["summary", "运行汇总"],
            ["displays", "组件展示"],
            ["outputs", "流程输出"],
          ].map(([id, title]) => (
            <button
              className={tab === id ? "active" : ""}
              key={id}
              onClick={() => setTab(id)}
            >
              {title}
            </button>
          ))}
        </div>
        {(result?.error || final?.error || final?.persistenceError) && (
          <div className="error-banner">
            {result?.error || final?.error || final?.persistenceError}
          </div>
        )}
        {tab === "outputs" ? (
          <pre className="response-code">
            {JSON.stringify(
              {
                output: final?.output ?? null,
                response: final?.response ?? null,
              },
              null,
              2,
            )}
          </pre>
        ) : tab === "displays" ? (
          <div className="flow-displays">
            {Object.entries(flowResults(snapshot))
              .filter(([, r]) => r.meta?.display)
              .map(([id, r]) => (
                <section key={id}>
                  <h3>{r.meta.label || id}</h3>
                  <FlowDisplay display={r.meta.display} />
                </section>
              ))}
            {!Object.values(flowResults(snapshot)).some(
              (r) => r.meta?.display,
            ) && <p className="muted">添加并运行 Display 节点后查看展示。</p>}
          </div>
        ) : tab === "timeline" ? (
          <div className="timeline">
            {events.map((e: Data, i: number) => (
              <button
                key={e.sequence}
                className={`timeline-row ${step === i ? "active" : ""}`}
                onClick={() => {
                  setStep(i);
                  setTab("json");
                }}
              >
                <span className={`status ${e.status}`}>
                  {stateLabels[e.status]}
                </span>
                <span className="mono">
                  {(e.nodeId || e.requestId).slice(0, 18)}
                </span>
                <span>迭代 {JSON.stringify(e.path)}</span>
                <span>{e.result.elapsedMs ?? "—"} ms</span>
              </button>
            ))}
            {events.length === 0 && <p className="muted">尚无已完成的步骤。</p>}
          </div>
        ) : tab === "logs" ? (
          <ConsolePanel snapshot={snapshot} />
        ) : tab === "tests" ? (
          <div className="test-list">
            {tests.length === 0 ? (
              <p className="muted">此步骤没有断言。</p>
            ) : (
              tests.map((t: Data, i: number) => (
                <div key={i}>
                  <span className={t.passed ? "success" : "error"}>
                    {t.passed ? "✓" : "×"}
                  </span>{" "}
                  {t.name}
                  {t.error && <pre>{t.error}</pre>}
                </div>
              ))
            )}
          </div>
        ) : (
          <pre className="response-code">
            {tab === "raw"
              ? (result?.raw ?? "此步骤没有原始响应")
              : JSON.stringify(
                  tab === "headers"
                    ? result?.meta?.headers || []
                    : tab === "summary"
                      ? (final ?? { status: snapshot.status })
                      : (result?.data ?? null),
                  null,
                  2,
                )}
          </pre>
        )}
      </div>
    </section>
  );
}
