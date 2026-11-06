import type { Data } from "./model";
import { linePoints } from "./flowPresentation";
const text = (value: unknown) =>
  typeof value === "string"
    ? value
    : (JSON.stringify(value, null, 2) ?? "null");
export function FlowDisplay({
  display,
  compact = false,
}: {
  display: Data;
  compact?: boolean;
}) {
  const value = display.value,
    mode = display.mode;
  if (mode === "boolean")
    return (
      <div className={`display-boolean ${value ? "yes" : "no"}`} role="status">
        {value ? "✓ true" : "× false"}
      </div>
    );
  if (mode === "table") {
    const rows = Array.isArray(value) ? value : [],
      limit = compact ? 5 : 200;
    const keys = Array.from(
      new Set(rows.slice(0, limit).flatMap((r: Data) => Object.keys(r))),
    ).slice(0, 32);
    return (
      <div className="display-table">
        <table>
          <thead>
            <tr>
              {keys.map((k) => (
                <th key={k}>{k}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.slice(0, limit).map((r: Data, i: number) => (
              <tr key={i}>
                {keys.map((k) => (
                  <td key={k}>{text(r[k]).slice(0, 500)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {!rows.length && <p>空数组</p>}
        {rows.length > limit && (
          <small>
            预览 {limit} / {rows.length} 行；完整数据见 JSON
          </small>
        )}
      </div>
    );
  }
  if (mode === "line") {
    const all = linePoints(value, display.xKey, display.yKey),
      points = all.slice(0, compact ? 80 : 500);
    if (!points.length) return <p className="muted">暂无数据</p>;
    const ys = points.map((p) => p.y),
      min = Math.min(...ys),
      max = Math.max(...ys),
      scale = Math.max(Math.abs(min), Math.abs(max), 1),
      range = max / scale - min / scale || 1;
    const x = (i: number) =>
      30 + (points.length === 1 ? 130 : (i * 260) / (points.length - 1));
    const y = (v: number) => 130 - ((v / scale - min / scale) * 100) / range;
    return (
      <div className="display-chart">
        <svg
          viewBox="0 0 320 170"
          role="img"
          aria-label={`折线图，${points.length} 个数据点`}
        >
          <path d="M30 20V140H300" fill="none" stroke="#cbdad5" />
          <polyline
            points={points.map((p, i) => `${x(i)},${y(p.y)}`).join(" ")}
            fill="none"
            stroke="#16816d"
            strokeWidth="2"
          />
          {points.map((p, i) => (
            <circle key={i} cx={x(i)} cy={y(p.y)} r="3" fill="#16816d">
              <title>
                {p.x}: {p.y}
              </title>
            </circle>
          ))}
          <text x="30" y="160">
            {points[0].x.slice(0, 18)}
          </text>
          <text x="298" y="160" textAnchor="end">
            {points[points.length - 1].x.slice(0, 18)}
          </text>
          <text x="3" y="25">
            {max.toPrecision(3)}
          </text>
          <text x="3" y="137">
            {min.toPrecision(3)}
          </text>
        </svg>
        {all.length > points.length && (
          <small>
            预览 {points.length} / {all.length} 个点
          </small>
        )}
      </div>
    );
  }
  return (
    <pre className="display-json">
      {(mode === "json"
        ? (JSON.stringify(value, null, 2) ?? "null")
        : text(value)
      ).slice(0, compact ? 2000 : 30000)}
    </pre>
  );
}
