import type { Data, Request } from "./model";

// Parse arguments only. Imported text is never passed to a shell.
export function curlTokens(source: string): string[] {
  const tokens: string[] = [];
  let token = "",
    quote = "",
    started = false;
  for (let i = 0; i < source.length; i++) {
    const c = source[i];
    if (!quote && c === "$" && source[i + 1] === "'")
      throw Error("暂不支持 $'…' 转义字符串，请改用普通单引号或双引号");
    if (
      c === "\\" &&
      quote !== "'" &&
      source[i + 1] === "\r" &&
      source[i + 2] === "\n"
    ) {
      i += 2;
      continue;
    }
    if (c === "\\" && source[i + 1] === "\n" && quote !== "'") {
      i++;
      continue;
    }
    if (quote) {
      if (c === quote) quote = "";
      else if (
        c === "\\" &&
        quote === '"' &&
        ['"', "\\", "$", "`"].includes(source[i + 1])
      )
        token += source[++i];
      else token += c;
    } else if (c === "'" || c === '"') {
      quote = c;
      started = true;
    } else if (/\s/.test(c)) {
      if (started) tokens.push(token);
      token = "";
      started = false;
    } else if (c === "\\") {
      if (++i >= source.length) throw Error("命令末尾的转义符不完整");
      token += source[i];
      started = true;
    } else if (";|&<>`".includes(c) || (c === "$" && source[i + 1] === "("))
      throw Error("只支持单条 cURL 命令，不支持管道、重定向或命令替换");
    else {
      token += c;
      started = true;
    }
  }
  if (quote) throw Error("引号未闭合");
  if (started) tokens.push(token);
  return tokens;
}
const pair = (key: string, value: string, type = "text") => ({
  key,
  value,
  type,
  enabled: true,
});
const split = (s: string, separator: string): [string, string] => {
  const i = s.indexOf(separator);
  if (i < 0) throw Error(`参数缺少 ${separator}：${s}`);
  return [s.slice(0, i), s.slice(i + separator.length)];
};
export function importCurl(source: string): { config: Data; secret?: string } {
  const args = curlTokens(source);
  if (!/^curl(?:\.exe)?$/i.test(args.shift() || ""))
    throw Error(
      "命令必须以 curl 或 curl.exe 开头（支持 POSIX 引号及反斜杠续行）",
    );
  const config: Data = {
    method: "GET",
    url: "",
    headers: [],
    query: [],
    auth: { type: "none" },
    body: { type: "none", text: "", fields: [] },
    timeoutMs: 30000,
    followRedirects: false,
  };
  let method = "",
    head = false,
    get = false,
    secret: string | undefined;
  const raw: string[] = [],
    fields: Data[] = [],
    multipart: Data[] = [];
  let file: string | undefined;
  for (let i = 0; i < args.length; i++) {
    let option = args[i],
      inline: string | undefined;
    if (option.startsWith("--") && option.includes("="))
      [option, inline] = split(option, "=");
    else if (/^-[XHduFbmoAew]/.test(option) && option.length > 2) {
      inline = option.slice(2);
      option = option.slice(0, 2);
    }
    const value = () => {
      const v = inline ?? args[++i];
      if (v === undefined) throw Error(`${option} 缺少参数`);
      return v;
    };
    switch (option) {
      case "-X":
      case "--request":
        method = value().toUpperCase();
        break;
      case "--url":
        if (config.url) throw Error("一次只能导入一个 URL");
        config.url = value();
        break;
      case "-H":
      case "--header": {
        const h = value();
        if (h.startsWith("@")) throw Error("暂不支持从文件读取 Headers");
        const [k, v] = split(h, ":");
        config.headers.push(pair(k.trim(), v.trimStart()));
        break;
      }
      case "-b":
      case "--cookie": {
        const v = value();
        if (!v.includes("="))
          throw Error("Cookie 文件暂不支持，请使用 name=value");
        config.headers.push(pair("Cookie", v));
        break;
      }
      case "-u":
      case "--user": {
        const [username, password] = split(value(), ":");
        config.auth = { type: "basic", username };
        secret = password;
        break;
      }
      case "--oauth2-bearer":
        config.auth = { type: "bearer" };
        secret = value();
        break;
      case "-A":
      case "--user-agent":
        config.headers.push(pair("User-Agent", value()));
        break;
      case "-e":
      case "--referer":
        config.headers.push(pair("Referer", value()));
        break;
      case "-L":
      case "--location":
        config.followRedirects = true;
        break;
      case "-I":
      case "--head":
        head = true;
        break;
      case "-G":
      case "--get":
        get = true;
        break;
      case "-m":
      case "--max-time": {
        const n = Number(value());
        if (!Number.isFinite(n) || n <= 0) throw Error("超时必须为正数（秒）");
        config.timeoutMs = n * 1000;
        break;
      }
      case "--json": {
        const v = value();
        if (v.startsWith("@"))
          throw Error("--json @文件暂不支持，请粘贴 JSON 内容");
        JSON.parse(v);
        raw.push(v);
        config.headers.push(
          pair("Content-Type", "application/json"),
          pair("Accept", "application/json"),
        );
        break;
      }
      case "-d":
      case "--data":
      case "--data-ascii":
      case "--data-raw":
      case "--data-binary": {
        const v = value();
        if (option !== "--data-raw" && v.startsWith("@")) {
          if (option !== "--data-binary" || v === "@-")
            throw Error("文件请求体仅支持 --data-binary @路径，不支持标准输入");
          file = v.slice(1);
        } else raw.push(v);
        break;
      }
      case "--data-urlencode": {
        const [k, v] = split(value(), "=");
        if (k.includes("@")) throw Error("不支持从文件读取表单字段");
        fields.push(pair(decodeURIComponent(k.replace(/\+/g, " ")), v));
        break;
      }
      case "-F":
      case "--form":
      case "--form-string": {
        const [k, v] = split(value(), "=");
        if (
          option !== "--form-string" &&
          (v.includes(";") ||
            v.startsWith("<") ||
            (v.startsWith("@") && v.includes(",")))
        )
          throw Error("暂不支持 multipart 修饰符或多文件字段");
        const isFile = option !== "--form-string" && v.startsWith("@");
        if (isFile && v === "@-") throw Error("不支持标准输入文件");
        multipart.push(
          pair(
            k,
            isFile ? v.slice(1).replace(/^"(.*)"$/, "$1") : v,
            isFile ? "file" : "text",
          ),
        );
        break;
      }
      case "-s":
      case "--silent":
      case "-S":
      case "--show-error":
      case "-sS":
      case "--compressed":
        break;
      default:
        if (option.startsWith("-"))
          throw Error(`暂不支持 cURL 选项：${option}`);
        if (config.url) throw Error("一次只能导入一个 URL");
        config.url = option;
    }
  }
  if (!/^https?:\/\//i.test(config.url)) throw Error("需要 HTTP(S) URL");
  const modes = [
    raw.length > 0,
    fields.length > 0,
    multipart.length > 0,
    file !== undefined,
  ].filter(Boolean).length;
  if (modes > 1) throw Error("暂不支持混用不同类型的请求体选项");
  if (get && (multipart.length || file !== undefined))
    throw Error("--get 不支持文件或 multipart 请求体");
  const contentType =
    config.headers.find((h: Data) => h.key.toLowerCase() === "content-type")
      ?.value || "";
  if (raw.length) {
    const text = raw.join("&");
    if (get)
      config.query = [...new URLSearchParams(text)].map(([k, v]) => pair(k, v));
    else {
      config.body = { type: "text", text };
      if (/^application\/(?:[\w.+-]+\+)?json(?:;|$)/i.test(contentType)) {
        try {
          JSON.parse(text);
          config.body.type = "json";
        } catch {
          /* Preserve intentionally invalid JSON as raw text. */
        }
      }
      if (
        !config.headers.some(
          (h: Data) => h.key.toLowerCase() === "content-type",
        )
      )
        config.headers.push(
          pair("Content-Type", "application/x-www-form-urlencoded"),
        );
    }
  } else if (fields.length) {
    if (get) config.query = fields;
    else config.body = { type: "form", fields };
  } else if (multipart.length)
    config.body = { type: "multipart", fields: multipart };
  else if (file !== undefined) config.body = { type: "file", path: file };
  if (
    file !== undefined &&
    !config.headers.some((h: Data) => h.key.toLowerCase() === "content-type")
  )
    config.headers.push(
      pair("Content-Type", "application/x-www-form-urlencoded"),
    );
  config.headers = config.headers.filter((h: Data) => h.value !== "");
  const bearer = config.headers.find(
    (h: Data) =>
      h.key.toLowerCase() === "authorization" && /^Bearer /i.test(h.value),
  );
  if (bearer && config.auth.type === "none") {
    config.auth = { type: "bearer" };
    secret = bearer.value.slice(7);
    config.headers = config.headers.filter((h: Data) => h !== bearer);
  }
  config.method =
    method || (head ? "HEAD" : get ? "GET" : modes ? "POST" : "GET");
  if (!/^[A-Z!#$%&'*+.^_`|~-]+$/.test(config.method))
    throw Error("HTTP 方法无效");
  return { config, secret };
}

export type ExportFormat = "curl" | "powershell" | "bash";
export function exportHttp(r: Request, format: ExportFormat): string {
  if (r.kind !== "http") throw Error("仅支持导出 HTTP 请求");
  const q = posixQuote;
  const args: string[][] = [],
    add = (...a: string[]) => args.push(a);
  let url = r.url || "";
  const query = (r.query || [])
    .filter((p: Data) => p.enabled !== false)
    .map(
      (p: Data) =>
        `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`,
    )
    .join("&");
  if (query) {
    const i = url.indexOf("#"),
      hash = i < 0 ? "" : url.slice(i);
    url = i < 0 ? url : url.slice(0, i);
    url += (url.includes("?") ? "&" : "?") + query + hash;
  }
  if (format === "powershell") return exportPowerShell(r, url);
  if (format === "bash") return exportWget(r, url);
  if (r.method === "HEAD") add("--head");
  else add("--request", r.method || "GET");
  add("--url", url);
  if (r.followRedirects !== false) add("--location");
  add("--max-time", String((r.timeoutMs ?? 30000) / 1000));
  const headers = (r.headers || []).filter((h: Data) => h.enabled !== false);
  const b = r.body || { type: "none" };
  for (const h of headers) add("--header", `${h.key}: ${h.value}`);
  if (r.auth?.type === "basic")
    add("--user", `${r.auth.username || ""}:{{secret:${r.auth.secretId}}}`);
  if (r.auth?.type === "bearer")
    add("--header", `Authorization: Bearer {{secret:${r.auth.secretId}}}`);
  if (
    b.type === "json" &&
    !headers.some((h: Data) => h.key.toLowerCase() === "content-type")
  )
    add("--header", "Content-Type: application/json");
  if (["json", "text"].includes(b.type)) {
    if (
      !headers.some((h: Data) => h.key.toLowerCase() === "content-type") &&
      b.type === "text"
    )
      add("--header", "Content-Type:");
    add("--data-raw", b.text || "");
  } else if (b.type === "file") {
    if (!headers.some((h: Data) => h.key.toLowerCase() === "content-type"))
      add("--header", "Content-Type:");
    add("--data-binary", `@${b.path || ""}`);
  } else if (["form", "multipart"].includes(b.type))
    for (const f of (b.fields || []).filter((f: Data) => f.enabled !== false)) {
      if (b.type === "form")
        add("--data-urlencode", `${encodeURIComponent(f.key)}=${f.value}`);
      else if (f.type === "file") {
        if (/[";\r\n]/.test(f.value))
          throw Error("文件路径包含无法导出的 multipart 特殊字符");
        add("--form", `${f.key}=@"${f.value}"`);
      } else add("--form-string", `${f.key}=${f.value}`);
    }
  return (
    "curl \\\n  " + args.map((a) => a.map(q).join(" ")).join(" \\\n  ") + "\n"
  );
}

const posixQuote = (s: string) =>
  "'" + String(s).split("'").join("'\\''") + "'";
const psQuote = (s: string) => "'" + String(s).split("'").join("''") + "'";
const enabled = (pairs: Data[] = []) =>
  pairs.filter((p) => p.enabled !== false);
const encodedForm = (fields: Data[] = []) =>
  new URLSearchParams(
    enabled(fields).map((f) => [String(f.key), String(f.value)]),
  ).toString();
function transferHeaders(r: Request): Data[] {
  let headers = enabled(r.headers).map((h) => ({ ...h }));
  const replace = (key: string, value: string) => {
    headers = headers.filter((h) => h.key.toLowerCase() !== key.toLowerCase());
    headers.push({ key, value });
  };
  if (r.auth?.type === "bearer")
    replace("Authorization", `Bearer {{secret:${r.auth.secretId}}}`);
  if (r.body?.type === "json") replace("Content-Type", "application/json");
  if (r.body?.type === "form")
    replace("Content-Type", "application/x-www-form-urlencoded");
  return headers;
}

function exportPowerShell(r: Request, url: string): string {
  const q = psQuote,
    b = r.body || { type: "none" };
  const lines = [
    "#Requires -Version 7.0",
    "$ErrorActionPreference = 'Stop'",
    "$headers = @{}",
  ];
  const headers = transferHeaders(r);
  const groups = new Map<string, Data[]>();
  for (const h of headers) {
    const k = h.key.toLowerCase();
    groups.set(k, [...(groups.get(k) || []), h]);
  }
  for (const group of groups.values()) {
    if (group[0].key.toLowerCase() === "content-type") continue;
    lines.push(
      `$headers[${q(group[0].key)}] = ${group.length === 1 ? q(group[0].value) : `@(${group.map((h) => q(h.value)).join(", ")})`}`,
    );
  }
  if (r.auth?.type === "basic")
    lines.push(
      `$headers['Authorization'] = 'Basic ' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes(${q(`${r.auth.username || ""}:{{secret:${r.auth.secretId}}}`)}))`,
    );
  lines.push(
    "$request = @{",
    `  Uri = ${q(url)}`,
    `  CustomMethod = ${q(r.method || "GET")}`,
    "  Headers = $headers",
    `  TimeoutSec = ${Math.max(1, Math.ceil((r.timeoutMs ?? 30000) / 1000))}`,
    `  MaximumRedirection = ${r.followRedirects === false ? 0 : 10}`,
    "  SkipHttpErrorCheck = $true",
    "}",
  );
  const contentType = groups.get("content-type");
  if (contentType?.length)
    lines.push(
      `$request.ContentType = ${q(contentType[contentType.length - 1].value)}`,
    );
  if (["json", "text", "form"].includes(b.type))
    lines.push(
      `$request.Body = [Text.Encoding]::UTF8.GetBytes(${q(b.type === "form" ? encodedForm(b.fields) : b.text || "")})`,
    );
  else if (b.type === "file")
    lines.push(`$request.InFile = ${q(b.path || "")}`);
  if (b.type === "multipart") {
    lines.push(
      "$multipart = [System.Net.Http.MultipartFormDataContent]::new()",
      "try {",
    );
    for (const f of enabled(b.fields)) {
      if (f.type === "file")
        lines.push(
          `  $part = [System.Net.Http.StreamContent]::new([IO.File]::OpenRead(${q(f.value)}))`,
          `  $multipart.Add($part, ${q(f.key)}, [IO.Path]::GetFileName(${q(f.value)}))`,
        );
      else
        lines.push(
          `  $multipart.Add([System.Net.Http.StringContent]::new(${q(f.value)}, [Text.Encoding]::UTF8), ${q(f.key)})`,
        );
    }
    lines.push(
      "  $request.Body = $multipart",
      "  $response = Invoke-WebRequest @request",
      "  $response.Content",
      "} finally {",
      "  $multipart.Dispose()",
      "}",
    );
  } else
    lines.push("$response = Invoke-WebRequest @request", "$response.Content");
  return lines.join("\n") + "\n";
}

function exportWget(r: Request, url: string): string {
  const q = posixQuote,
    b = r.body || { type: "none" };
  const lines = [
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    "",
    "# Requires GNU Wget (not BusyBox wget). Timeout is an inactivity timeout.",
  ];
  const args: string[][] = [
    ["--output-document=-"],
    ["--content-on-error"],
    ["--tries=1"],
    [`--timeout=${(r.timeoutMs ?? 30000) / 1000}`],
    [`--max-redirect=${r.followRedirects === false ? 0 : 10}`],
    [`--method=${r.method || "GET"}`],
  ];
  const headers = transferHeaders(r);
  if (r.auth?.type === "basic") {
    lines.push(
      `basic_auth=$(printf '%s' ${q(`${r.auth.username || ""}:{{secret:${r.auth.secretId}}}`)} | base64 | tr -d '\\r\\n')`,
    );
  }
  for (const h of headers)
    if (
      !(r.auth?.type === "basic" && h.key.toLowerCase() === "authorization") &&
      !(b.type === "multipart" && h.key.toLowerCase() === "content-type")
    )
      args.push([`--header=${h.key}: ${h.value}`]);
  if (["json", "text", "form"].includes(b.type))
    args.push([
      `--body-data=${b.type === "form" ? encodedForm(b.fields) : b.text || ""}`,
    ]);
  else if (b.type === "file") args.push([`--body-file=${b.path || ""}`]);
  if (b.type === "multipart") {
    const boundary = "----Boostman" + crypto.randomUUID().replace(/-/g, "");
    const disposition = (s: string) => {
      if (/[\r\n]/.test(s)) throw Error("multipart 字段名和文件名不能包含换行");
      return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
    };
    lines.push(
      "body_file=$(mktemp)",
      "trap 'rm -f -- \"$body_file\"' EXIT",
      "{",
    );
    for (const f of enabled(b.fields)) {
      const name = disposition(f.key),
        filename =
          f.type === "file"
            ? disposition(String(f.value).split(/[\\/]/).pop() || "file")
            : "";
      lines.push(
        `  printf '%s\\r\\n' ${q(`--${boundary}`)} ${q(`Content-Disposition: form-data; name="${name}"${f.type === "file" ? `; filename="${filename}"` : ""}`)}`,
      );
      if (f.type === "file")
        lines.push(
          "  printf '%s\\r\\n' 'Content-Type: application/octet-stream'",
        );
      lines.push(
        "  printf '\\r\\n'",
        f.type === "file"
          ? `  cat -- ${q(f.value)}`
          : `  printf '%s' ${q(f.value)}`,
        "  printf '\\r\\n'",
      );
    }
    lines.push(
      `  printf '%s\\r\\n' ${q(`--${boundary}--`)}`,
      '} > "$body_file"',
    );
    args.push([
      `--header=Content-Type: multipart/form-data; boundary=${boundary}`,
    ]);
  }
  const rendered = args.map((a) => a.map(q).join(" "));
  if (r.auth?.type === "basic")
    rendered.push('"--header=Authorization: Basic $basic_auth"');
  if (b.type === "multipart") rendered.push('"--body-file=$body_file"');
  rendered.push("--", q(url));
  lines.push("wget \\\n  " + rendered.join(" \\\n  "));
  return lines.join("\n") + "\n";
}
