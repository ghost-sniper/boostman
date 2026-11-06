"use strict";
globalThis.AsyncFunction = Object.getPrototypeOf(
  async function () {},
).constructor;
globalThis.__pending = [];
globalThis.__tests = [];
globalThis.__logs = [];
const unsupported = (name, obj) =>
  new Proxy(obj, {
    get(o, k) {
      if (k === "then") return undefined;
      if (typeof k === "symbol" || k in o) return o[k];
      throw new Error(`不支持的 API: ${name}.${k}`);
    },
  });
const scope = (name, values, writable = true) =>
  unsupported(name, {
    get: (k) => values[k],
    has: (k) => Object.prototype.hasOwnProperty.call(values, k),
    toObject: () => ({ ...values }),
    set(k, v) {
      if (!writable) throw Error("迭代数据只读");
      values[k] = v;
    },
    unset(k) {
      if (!writable) throw Error("迭代数据只读");
      delete values[k];
    },
    clear() {
      if (!writable) throw Error("迭代数据只读");
      for (const k of Object.keys(values)) delete values[k];
    },
  });
function headers(list = []) {
  return unsupported("headers", {
    get: (k) =>
      list.find(
        (x) => x.key.toLowerCase() === k.toLowerCase() && x.enabled !== false,
      )?.value,
    has: (k) => list.some((x) => x.key.toLowerCase() === k.toLowerCase()),
    add(p) {
      list.push(
        typeof p === "string"
          ? { key: p.split(":")[0], value: p.slice(p.indexOf(":") + 1).trim() }
          : p,
      );
    },
    upsert(p) {
      this.remove(p.key);
      list.push(p);
    },
    remove(k) {
      for (let i = list.length - 1; i >= 0; i--)
        if (list[i].key.toLowerCase() === k.toLowerCase()) list.splice(i, 1);
    },
    toJSON: () => list,
    each: (fn) => list.forEach(fn),
    all: () => list,
  });
}
function response(r) {
  if (!r) return undefined;
  let h = headers(r.meta.headers);
  let result = {
    code: r.meta.statusCode,
    status: r.meta.statusText || String(r.meta.statusCode),
    responseTime: r.elapsedMs,
    headers: h,
    json: () => {
      if (r.raw === undefined) return JSON.parse(JSON.stringify(r.data));
      if (r.meta.encoding !== "utf8") throw Error("响应不是文本");
      return JSON.parse(r.raw);
    },
    text: () => r.raw ?? JSON.stringify(r.data),
    to: {
      have: {
        status(code) {
          if (result.code !== code)
            throw Error(`状态码 ${result.code} != ${code}`);
        },
      },
    },
  };
  return unsupported("pm.response", result);
}
function deepEqual(a, b) {
  if (a === b) return true;
  if (
    !a ||
    !b ||
    typeof a !== "object" ||
    typeof b !== "object" ||
    Array.isArray(a) !== Array.isArray(b)
  )
    return false;
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length &&
    keys.every(
      (k) =>
        Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]),
    )
  );
}
function expect(value) {
  let negate = false,
    deep = false;
  const chain = {};
  const test = (ok, label) => {
    if (negate ? ok : !ok) throw Error(label);
    negate = false;
    deep = false;
    return proxy;
  };
  const methods = {
    equal: (v) =>
      test(
        deep ? deepEqual(value, v) : value === v,
        `断言相等失败: ${JSON.stringify(value)} / ${JSON.stringify(v)}`,
      ),
    eql: (v) => test(deepEqual(value, v), "深度相等失败"),
    include: (v) =>
      test(
        typeof value === "string"
          ? value.includes(v)
          : Array.isArray(value)
            ? value.some((item) => (deep ? deepEqual(item, v) : item === v))
            : value && typeof value === "object"
              ? Object.keys(v).every((k) =>
                  deep ? deepEqual(value[k], v[k]) : value[k] === v[k],
                )
              : false,
        "包含断言失败",
      ),
    property: (k, ...args) => {
      const next = value?.[k];
      const result = test(
        value != null &&
          k in Object(value) &&
          (args.length === 0 ||
            (deep ? deepEqual(next, args[0]) : next === args[0])),
        `属性断言失败: ${k}`,
      );
      value = next;
      return result;
    },
    a: (t) =>
      test(
        (Array.isArray(value)
          ? "array"
          : value === null
            ? "null"
            : typeof value) === t,
        `类型断言失败: ${t}`,
      ),
    lengthOf: (n) => test(value?.length === n, `长度断言失败: ${n}`),
  };
  const proxy = new Proxy(chain, {
    get(_, k) {
      if (k === "then") return undefined;
      if (["to", "be", "have", "and", "that", "which", "is"].includes(k))
        return proxy;
      if (k === "not") {
        negate = !negate;
        return proxy;
      }
      if (k === "deep") {
        deep = true;
        return proxy;
      }
      if (k === "an") return methods.a;
      if (k === "equals") return methods.equal;
      if (k === "contain") return methods.include;
      if (k === "length") return methods.lengthOf;
      if (k === "ok") return test(!!value, "truthy 断言失败");
      if (k === "true" || k === "false" || k === "null" || k === "undefined")
        return test(
          value ===
            { true: true, false: false, null: null, undefined: undefined }[k],
          `${k} 断言失败`,
        );
      if (k in methods) return methods[k];
      throw Error(`不支持的 pm.expect API: ${String(k)}`);
    },
  });
  return proxy;
}
const raw = __seed.request;
raw.headers = raw.headers || [];
raw.body = raw.body || { type: "none" };
const request = unsupported("pm.request", {
  method: raw.method || "GET",
  url: {
    value: raw.url || "",
    update(s) {
      this.value = String(s);
    },
    toString() {
      return this.value;
    },
  },
  headers: headers(raw.headers),
  body: {
    mode:
      raw.body.type === "text" || raw.body.type === "json"
        ? "raw"
        : raw.body.type,
    raw: raw.body.text || "",
    update(v) {
      if (typeof v === "string") {
        this.mode = "raw";
        this.raw = v;
      } else Object.assign(this, v);
    },
  },
});
globalThis.__exportRequest = () => {
  raw.method = request.method;
  raw.url = String(request.url);
  if (request.body.mode === "raw") {
    raw.body.type = raw.body.type === "json" ? "json" : "text";
    raw.body.text = request.body.raw;
  }
  return raw;
};
function resolved() {
  return {
    ...__seed.collection,
    ...__seed.environment,
    ...__seed.iteration,
    ...__seed.locals,
  };
}
const variables = scope("pm.variables", __seed.locals);
variables.get = (k) => resolved()[k];
variables.has = (k) => k in resolved();
variables.toObject = resolved;
variables.replaceIn = (s) =>
  String(s).replace(/\{\{([^{}]+)\}\}/g, (_, k) => {
    if (!(k in resolved())) throw Error(`变量不存在: ${k}`);
    const v = resolved()[k];
    return typeof v === "string" ? v : JSON.stringify(v);
  });
globalThis.pm = unsupported("pm", {
  variables,
  environment: scope("pm.environment", __seed.environment),
  collectionVariables: scope("pm.collectionVariables", __seed.collection),
  iterationData: scope("pm.iterationData", __seed.iteration, false),
  request,
  response: response(__seed.response),
  info: {
    iteration: __seed.iterationIndex || 0,
    requestId: raw.id,
    requestName: raw.name,
  },
  expect,
  test(name, fn) {
    try {
      const r = fn();
      if (r && typeof r.then === "function") {
        __pending.push(
          r.then(
            () => __tests.push({ name, passed: true }),
            (e) => __tests.push({ name, passed: false, error: String(e) }),
          ),
        );
      } else __tests.push({ name, passed: true });
    } catch (e) {
      __tests.push({ name, passed: false, error: String(e) });
    }
  },
  sendRequest(input, cb) {
    const task = Promise.resolve().then(() => {
      const r = typeof input === "string" ? { url: input } : input;
      const req = {
        ...r,
        url: String(r.url),
        headers: Array.isArray(r.header) ? r.header : r.headers || [],
      };
      if (r.body?.mode === "raw") req.body = { type: "text", text: r.body.raw };
      const out = JSON.parse(__hostSend(JSON.stringify(req)));
      if (!out.ok) throw Error(out.error);
      return response(out.response);
    });
    const delivered = cb
      ? task.then(
          (r) => {
            cb(null, r);
            return r;
          },
          (e) => {
            cb(e, null);
            return null;
          },
        )
      : task;
    __pending.push(delivered);
    return delivered;
  },
});
function consoleValue(value) {
  if (value === undefined) return "undefined";
  if (value instanceof Error) return String(value);
  const seen = new WeakSet();
  try {
    const encoded = JSON.stringify(value, (_, item) => {
      if (typeof item === "bigint") return `${item}n`;
      if (item && typeof item === "object") {
        if (seen.has(item)) return "[Circular]";
        seen.add(item);
      }
      return item;
    });
    return encoded === undefined ? String(value) : JSON.parse(encoded);
  } catch {
    return String(value);
  }
}
function writeConsole(level, args) {
  const values = args.map(consoleValue);
  const entry = {
    level,
    args: values,
    message: values
      .map((v) => (typeof v === "string" ? v : JSON.stringify(v)))
      .join(" "),
    timestamp: new Date().toISOString(),
  };
  if (__logs.length >= 2000) __logs.shift();
  __logs.push(entry);
  __hostLog(JSON.stringify(entry));
}
globalThis.console = unsupported(
  "console",
  Object.fromEntries(
    ["log", "info", "warn", "error", "debug"].map((level) => [
      level,
      (...args) => writeConsole(level, args),
    ]),
  ),
);
