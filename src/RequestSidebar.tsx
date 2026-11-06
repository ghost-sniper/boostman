import { useState, useEffect } from "react";
import type { Request, RequestFolder } from "./model";
import { folderPath, searchRequests } from "./requestFolders";
export function RequestSidebar({
  requests,
  folders,
  kind,
  onKind,
  selected,
  folderId,
  query,
  onRequest,
  onFolder,
  onAddRequest,
  onAddFolder,
  onImport,
  disabled,
}: {
  requests: Request[];
  folders: RequestFolder[];
  kind: Request["kind"];
  onKind: (kind: Request["kind"]) => void;
  selected: string;
  folderId: string | null;
  query: string;
  onRequest: (request: Request) => void;
  onFolder: (id: string | null) => void;
  onAddRequest: () => void;
  onAddFolder: () => void;
  onImport: () => void;
  disabled: boolean;
}) {
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const visible = searchRequests(requests, folders, kind, query);
  useEffect(() => {
    setClosed((old) => {
      const next = new Set(old),
        seen = new Set<string>();
      let id = folderId;
      while (id && !seen.has(id)) {
        seen.add(id);
        next.delete(id);
        id = folders.find((f) => f.id === id)?.parentId || null;
      }
      return next;
    });
  }, [folderId, folders]);
  const toggle = (id: string) =>
    setClosed((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const tree = (parent: string | null, depth = 0) => (
    <>
      {folders
        .filter(
          (f) =>
            f.kind === kind &&
            f.parentId === parent &&
            visible.folders.has(f.id),
        )
        .map((f) => (
          <div key={f.id}>
            <div
              className={`request-folder-row ${!selected && folderId === f.id ? "selected" : ""}`}
              style={{ paddingLeft: 10 + depth * 14 }}
            >
              <button
                className="folder-toggle"
                title={closed.has(f.id) ? "展开目录" : "折叠目录"}
                onClick={() => toggle(f.id)}
              >
                {!closed.has(f.id) || query ? "▾" : "▸"}
              </button>
              <button
                className="folder-name"
                title={folderPath(folders, f.id)}
                onClick={() => {
                  setClosed((old) => {
                    const next = new Set(old);
                    next.delete(f.id);
                    return next;
                  });
                  onFolder(f.id);
                }}
              >
                ▱ {f.name}
              </button>
            </div>
            {(!closed.has(f.id) || query) && tree(f.id, depth + 1)}
          </div>
        ))}
      {requests
        .filter(
          (r) =>
            r.kind === kind &&
            (r.folderId || null) === parent &&
            visible.requests.has(r.id),
        )
        .map((r) => (
          <button
            key={r.id}
            title={r.name}
            className={`sidebar-item ${selected === r.id ? "selected" : ""}`}
            style={{ paddingLeft: 14 + depth * 14 }}
            onClick={() => onRequest(r)}
          >
            <span
              className={`method ${kind === "sql" ? "sql" : (r.method || "").toLowerCase()}`}
            >
              {kind === "sql" ? "SQL" : r.method}
            </span>
            <span>{r.name}</span>
          </button>
        ))}
    </>
  );
  return (
    <>
      <div className="request-kind-tabs" role="tablist" aria-label="请求类型">
        {(["http", "sql"] as const).map((value) => (
          <button
            key={value}
            role="tab"
            aria-selected={kind === value}
            className={kind === value ? "active" : ""}
            onClick={() => onKind(value)}
          >
            {value === "http" ? "HTTP" : "SQL"}{" "}
            <small>{requests.filter((r) => r.kind === value).length}</small>
          </button>
        ))}
      </div>
      <div className="request-tree-tools">
        <button disabled={disabled} onClick={onAddRequest}>
          ＋ 新建{kind === "http" ? " HTTP" : " SQL"}
        </button>
        <button disabled={disabled} onClick={onAddFolder}>
          ＋ 目录
        </button>
        {kind === "http" && (
          <button disabled={disabled} onClick={onImport}>
            导入 cURL
          </button>
        )}
      </div>
      <div className="sidebar-items request-tree">
        <button
          className={`request-root ${!selected && !folderId ? "selected" : ""}`}
          onClick={() => onFolder(null)}
        >
          ⌂ 根目录
        </button>
        {tree(null)}
        {visible.requests.size === 0 && visible.folders.size === 0 && (
          <div className="sidebar-empty">
            {query
              ? "没有匹配的请求或目录"
              : `还没有 ${kind === "http" ? "HTTP" : "SQL"} 请求或目录`}
          </div>
        )}
      </div>
    </>
  );
}
