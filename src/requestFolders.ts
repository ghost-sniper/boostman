import type { Workspace, RequestFolder, Request } from "./model";
export function requestLabel(
  request: Request,
  folders: RequestFolder[],
): string {
  const path = folderPath(folders, request.folderId);
  return path ? `${path} / ${request.name}` : request.name;
}
export function folderPath(
  folders: RequestFolder[],
  id?: string | null,
): string {
  const names: string[] = [],
    seen = new Set<string>();
  let current = id;
  while (current) {
    if (seen.has(current)) break;
    seen.add(current);
    const folder = folders.find((f) => f.id === current);
    if (!folder) break;
    names.unshift(folder.name);
    current = folder.parentId;
  }
  return names.join(" / ");
}
export function descendants(folders: RequestFolder[], id: string): Set<string> {
  const result = new Set([id]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const f of folders) {
      if (f.parentId && result.has(f.parentId) && !result.has(f.id)) {
        result.add(f.id);
        changed = true;
      }
    }
  }
  return result;
}
export function folderChoices(
  folders: RequestFolder[],
  kind: Request["kind"],
  exclude?: string,
): RequestFolder[] {
  const excluded = exclude ? descendants(folders, exclude) : new Set<string>();
  return folders
    .filter((f) => f.kind === kind && !excluded.has(f.id))
    .sort((a, b) =>
      folderPath(folders, a.id).localeCompare(folderPath(folders, b.id)),
    );
}
export function moveRequest(
  workspace: Workspace,
  id: string,
  folderId: string | null,
): Workspace {
  const request = workspace.requests.find((r) => r.id === id);
  if (!request) throw Error("请求不存在");
  if (
    folderId &&
    !workspace.requestFolders.some(
      (f) => f.id === folderId && f.kind === request.kind,
    )
  )
    throw Error("请求只能移动到同类型目录");
  return {
    ...workspace,
    requests: workspace.requests.map((r) =>
      r.id === id ? { ...r, folderId } : r,
    ),
  };
}
export function moveFolder(
  workspace: Workspace,
  id: string,
  parentId: string | null,
): Workspace {
  const folder = workspace.requestFolders.find((f) => f.id === id);
  if (!folder) throw Error("目录不存在");
  if (
    parentId &&
    (!workspace.requestFolders.some(
      (f) => f.id === parentId && f.kind === folder.kind,
    ) ||
      descendants(workspace.requestFolders, id).has(parentId))
  )
    throw Error("不能跨类型移动目录或移入自身子目录");
  const next = {
    ...workspace,
    requestFolders: workspace.requestFolders.map((f) =>
      f.id === id ? { ...f, parentId } : f,
    ),
  };
  if (
    next.requestFolders.some(
      (f) => folderPathDepth(next.requestFolders, f.id) > 32,
    )
  )
    throw Error("目录嵌套不能超过 32 层");
  return next;
}
function folderPathDepth(folders: RequestFolder[], id: string): number {
  let n = 0,
    current: string | null = id;
  const seen = new Set<string>();
  while (current) {
    if (seen.has(current)) return Infinity;
    seen.add(current);
    n++;
    current = folders.find((f) => f.id === current)?.parentId || null;
  }
  return n;
}
export function removeFolder(workspace: Workspace, id: string): Workspace {
  const folder = workspace.requestFolders.find((f) => f.id === id);
  if (!folder) throw Error("目录不存在");
  return {
    ...workspace,
    requestFolders: workspace.requestFolders
      .filter((f) => f.id !== id)
      .map((f) =>
        f.parentId === id ? { ...f, parentId: folder.parentId } : f,
      ),
    requests: workspace.requests.map((r) =>
      r.folderId === id ? { ...r, folderId: folder.parentId } : r,
    ),
  };
}
export function searchRequests(
  requests: Request[],
  folders: RequestFolder[],
  kind: Request["kind"],
  query: string,
): { requests: Set<string>; folders: Set<string> } {
  const term = query.trim().toLocaleLowerCase(),
    result = { requests: new Set<string>(), folders: new Set<string>() };
  const includeAncestors = (id: string | null | undefined) => {
    const seen = new Set<string>();
    while (id && !seen.has(id)) {
      seen.add(id);
      result.folders.add(id);
      id = folders.find((f) => f.id === id)?.parentId;
    }
  };
  for (const r of requests) {
    if (
      r.kind === kind &&
      (!term ||
        `${r.name} ${folderPath(folders, r.folderId)}`
          .toLocaleLowerCase()
          .includes(term))
    ) {
      result.requests.add(r.id);
      includeAncestors(r.folderId);
    }
  }
  for (const f of folders) {
    if (
      f.kind === kind &&
      (!term || folderPath(folders, f.id).toLocaleLowerCase().includes(term))
    )
      includeAncestors(f.id);
  }
  return result;
}
