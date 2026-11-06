import { Field } from "./components";
import { databaseDrivers, changeConnectionDriver } from "./model";
import type { DatabaseConnection, DatabaseDriver } from "./model";
export function ConnectionEditor({
  connection: c,
  onChange,
  onTest,
  disabled,
}: {
  connection: DatabaseConnection;
  onChange: (c: DatabaseConnection) => void;
  onTest: () => void;
  disabled: boolean;
}) {
  const set = (patch: Partial<DatabaseConnection>) =>
    onChange({ ...c, ...patch });
  return (
    <div className="editor-pad settings-page">
      <div className="section-title">
        <div>
          <h2>数据库连接</h2>
          <p>统一配置连接信息，供 SQL 调试、集合和 Flows 使用。</p>
        </div>
        <span className="badge">DATABASE CONNECTION</span>
      </div>
      <div className="two-cols">
        <label className="field">
          <span>数据库类型</span>
          <select
            value={c.driver}
            onChange={(e) =>
              onChange(
                changeConnectionDriver(c, e.target.value as DatabaseDriver),
              )
            }
          >
            {Object.entries(databaseDrivers).map(([driver, meta]) => (
              <option key={driver} value={driver}>
                {meta.name}
              </option>
            ))}
          </select>
        </label>
        <Field label="主机" value={c.host} onChange={(host) => set({ host })} />
        <Field
          label={`端口（默认 ${databaseDrivers[c.driver].port}）`}
          type="number"
          value={c.port}
          onChange={(value) => set({ port: Number(value) })}
        />
        <Field
          label="数据库 / 服务名称"
          value={c.database}
          onChange={(database) => set({ database })}
        />
        <Field
          label="用户名"
          value={c.username}
          onChange={(username) => set({ username })}
        />
        <Field
          label="密码凭据 ID"
          value={c.secretId}
          onChange={(secretId) => set({ secretId })}
        />
      </div>
      <p className="muted">
        {c.driver === "oracle"
          ? "数据库 / 服务名称填写 Oracle 的 Service Name 或 SID。"
          : "数据库 / 服务名称填写要连接的数据库名称。"}
        在“环境与凭据”中保存密码，并填写相同的凭据 ID。
      </p>
      {c.driver === "oracle" && (
        <details open>
          <summary>Oracle 选项</summary>
          <label className="field">
            <span>服务标识类型</span>
            <select
              value={c.oracleMode || "service"}
              onChange={(e) =>
                set({ oracleMode: e.target.value as "service" | "sid" })
              }
            >
              <option value="service">Service Name</option>
              <option value="sid">SID</option>
            </select>
          </label>
          <p className="muted">
            需要与应用架构一致的 Oracle Instant Client Basic（建议 19c
            或更新版本），配置 PATH 后重启应用。参数使用
            :1、:2，查询示例：SELECT 1 FROM dual。
          </p>
        </details>
      )}
      <button disabled={disabled} onClick={onTest}>
        测试连接
      </button>
    </div>
  );
}
