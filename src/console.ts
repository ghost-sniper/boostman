import type { Data } from "./model";
export interface ConsoleEntry {
  logId: string;
  level: string;
  message: string;
  timestamp: string;
  nodeId: string;
  phase: string;
  path: number[];
  args: unknown[];
}
export function consoleEntries(snapshot: Data): ConsoleEntry[] {
  const final = snapshot.result?.logs;
  const events = (snapshot.events || []).filter(
    (event: Data) => event.type === "console",
  );
  const source = Array.isArray(final)
    ? final
    : events.length
      ? events
      : (snapshot.events || [])
          .filter((event: Data) => event.type === "step" && event.result)
          .flatMap((event: Data) =>
            (event.result.logs || []).map((entry: Data | string) =>
              typeof entry === "string"
                ? { message: entry, nodeId: event.nodeId, path: event.path }
                : { ...entry, nodeId: event.nodeId, path: event.path },
            ),
          );
  return source.map((entry: Data | string, index: number) => {
    const row = typeof entry === "string" ? { message: entry } : entry;
    return {
      logId: row.logId || `log-${index}`,
      level: row.level || "log",
      message: row.message ?? "",
      timestamp: row.timestamp || "",
      nodeId: row.nodeId || "",
      phase: row.phase || "",
      path: row.path || [],
      args: row.args || [],
    };
  });
}
