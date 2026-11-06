import { Field } from "./components";
import type { Request, RequestFolder } from "./model";
import { folderChoices, folderPath, descendants } from "./requestFolders";
export function RequestFolderEditor({
  folder,
  folders,
  requests,
  onRename,
  onMove,
  onDelete,
  onAddRequest,
  onAddFolder,
  disabled,
}: {
  folder: RequestFolder;
  folders: RequestFolder[];
  requests: Request[];
  onRename: (name: string) => void;
  onMove: (id: string | null) => void;
  onDelete: () => void;
  onAddRequest: () => void;
  onAddFolder: () => void;
  disabled: boolean;
}) {
  const nested = descendants(folders, folder.id),
    count = requests.filter((r) => r.folderId && nested.has(r.folderId)).length;
  return (
    <div className="editor-pad settings-page folder-editor">
      <div className="section-title">
        <div>
          <h2>▱ {folder.name || "未命名目录"}</h2>
          <p>
            {folder.kind === "http" ? "HTTP 请求" : "SQL 请求"} ·{" "}
            {folderPath(folders, folder.id)} · 包含 {count} 个请求
          </p>
        </div>
        <span className="badge">REQUEST FOLDER</span>
      </div>
      <fieldset disabled={disabled} className="editor-fieldset">
        <Field label="目录名称" value={folder.name} onChange={onRename} />
        <label className="field">
          <span>上级目录</span>
          <select
            value={folder.parentId || ""}
            onChange={(e) => onMove(e.target.value || null)}
          >
            <option value="">根目录</option>
            {folderChoices(folders, folder.kind, folder.id).map((f) => (
              <option key={f.id} value={f.id}>
                {folderPath(folders, f.id)}
              </option>
            ))}
          </select>
        </label>
        <div className="inline-add">
          <button className="primary" onClick={onAddRequest}>
            ＋ 新建{folder.kind === "http" ? " HTTP" : " SQL"} 请求
          </button>
          <button onClick={onAddFolder}>＋ 新建子目录</button>
        </div>
        <div className="folder-delete">
          <p className="muted">
            删除此目录会将其中的请求和子目录移到上级，请求及其引用会保留。
          </p>
          <button className="danger" onClick={onDelete}>
            删除目录并保留内容
          </button>
        </div>
      </fieldset>
    </div>
  );
}
