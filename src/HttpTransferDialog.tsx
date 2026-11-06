import { useEffect, useRef, useState } from "react";
import type { Data, Request } from "./model";
import { exportHttp, importCurl } from "./httpTransfer";
import type { ExportFormat } from "./httpTransfer";

export function HttpTransferDialog({
  request,
  onImport,
  onClose,
}: {
  request?: Request;
  onImport: (config: Data, secret?: string) => Promise<void>;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [source, setSource] = useState("");
  const [format, setFormat] = useState<ExportFormat>("curl");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  let output = "",
    exportError = "";
  if (request) {
    try {
      output = exportHttp(request, format);
    } catch (e) {
      exportError = String(e);
    }
  }
  const download = () => {
    const blob = new Blob([output], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob),
      a = document.createElement("a");
    a.href = url;
    a.download = `${(request?.name || "request").replace(/[<>:"/\\|?*\x00-\x1f]/g, "_")}.${format === "powershell" ? "ps1" : format === "bash" ? "sh" : "curl"}`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return (
    <dialog
      className="http-transfer-dialog"
      ref={dialog}
      aria-labelledby="http-transfer-title"
      onCancel={(e) => {
        if (busy) e.preventDefault();
        else onClose();
      }}
    >
      <div className="transfer-heading">
        <h2 id="http-transfer-title">
          {request ? "导出 HTTP 请求" : "导入 cURL 命令"}
        </h2>
        <button disabled={busy} aria-label="关闭" onClick={onClose}>
          关闭
        </button>
      </div>
      {request ? (
        <>
          <label className="field">
            <span>导出格式</span>
            <select
              value={format}
              onChange={(e) => {
                setFormat(e.target.value as ExportFormat);
                setNotice("");
              }}
            >
              <option value="curl">cURL 命令</option>
              <option value="powershell">
                PowerShell 脚本（Invoke-WebRequest）
              </option>
              <option value="bash">Bash 脚本（wget）</option>
            </select>
          </label>
          <p>
            导出当前静态配置，不执行脚本或解析环境变量。凭据库中的秘密以{" "}
            {"{{secret:凭据ID}}"} 占位，请在运行前替换；Headers
            和请求体中的明文会原样导出。PowerShell 使用
            Invoke-WebRequest（7.0+），Bash 使用 GNU
            Wget。脚本超时及重定向处理遵循各自工具的行为。
          </p>
        </>
      ) : (
        <p>
          粘贴单条 cURL 命令（POSIX
          引号、反斜杠续行）。支持方法、Headers、Cookie、Basic/Bearer、JSON/文本、表单及文件路径；不支持的参数会明确报错。导入将新建请求，不发送请求或读取文件。
        </p>
      )}
      <textarea
        className="code transfer-source"
        aria-label={request ? "导出脚本" : "cURL 命令"}
        value={request ? output : source}
        readOnly={!!request}
        disabled={busy}
        onChange={(e) => setSource(e.target.value)}
        placeholder={"curl 'http://localhost:9000/api/tags'"}
        spellCheck={false}
      />
      {(error || exportError) && (
        <p role="alert" className="danger">
          {error || exportError}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <div className="transfer-actions">
        {request ? (
          <>
            <button
              disabled={!!exportError}
              onClick={() => {
                void navigator.clipboard.writeText(output).then(
                  () => setNotice("已复制"),
                  () => setError("复制失败，请选中脚本手动复制"),
                );
              }}
            >
              复制脚本
            </button>
            <button
              className="primary"
              disabled={!!exportError}
              onClick={download}
            >
              下载文件
            </button>
          </>
        ) : (
          <button
            className="primary"
            disabled={busy || !source.trim()}
            onClick={async () => {
              setError("");
              setBusy(true);
              try {
                const parsed = importCurl(source);
                await onImport(parsed.config, parsed.secret);
                onClose();
              } catch (e) {
                setError(e instanceof Error ? e.message : String(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? "正在导入…" : "导入为新请求"}
          </button>
        )}
      </div>
    </dialog>
  );
}
