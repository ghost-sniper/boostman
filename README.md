# Boostman

基于 Tauri 2、React 和 Rust 的本地 API 调试工具。支持 HTTP(S) 请求、MySQL/PostgreSQL/Oracle 参数化 SQL、Collection Runner 和可视化 Flows。中文界面，无账户或云端依赖。

## 启动与验证

需要 Node.js、Rust 和 Tauri 的系统构建依赖；Windows 需要 MSVC 构建工具和 WebView2。

```powershell
npm install
npm run tauri dev
```

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run tauri build
```

`npm run dev` 只启动界面预览，不提供浏览器网络执行或工作区保存。实际请求、SQL、FQL 和脚本都通过桌面 Rust 核心执行。`package-lock.json` 和 `src-tauri/Cargo.lock` 固定依赖。

## GitHub Actions 构建

工作流 `.github/workflows/build.yml` 在推送 `main`、`v*` 标签、针对 `main` 的 PR 或手动运行时执行。使用 Node.js 24 和 Rust 1.99.1，先检查格式、前端类型与构建、连接配置及安装包校验测试、Rust 检查和核心测试，再以 Release O3 + LTO 构建安装包。

| 平台    | 构建目标（仅 x86_64）      | 安装包             | Actions Artifact          |
| ------- | -------------------------- | ------------------ | ------------------------- |
| Windows | `x86_64-pc-windows-msvc`   | MSI、NSIS 安装 EXE | `boostman-windows-x86_64` |
| Linux   | `x86_64-unknown-linux-gnu` | deb、rpm           | `boostman-linux-x86_64`   |
| macOS   | `x86_64-apple-darwin`      | DMG                | `boostman-macos-x86_64`   |

在仓库 Actions 的对应运行页面下载 Artifact，默认保留 14 天，每个平台包含 `SHA256SUMS.txt`。上传前检查所需包格式、程序的实际 CPU 架构，以及 deb/rpm 的架构元信息；不生成 ARM 或 Universal 包。macOS 明确使用 Intel runner，包采用 ad-hoc 签名，尚未配置 Apple 开发者证书或公证；Windows 安装包也未配置发布者签名。工作流不会自动创建 GitHub Release。

CI 不连接业务数据库，也不包含数据库密码；需外部服务的集成测试保持默认忽略。Oracle Instant Client 仍由使用 Oracle 功能的目标机器提供，不随安装包分发。

## 使用

1. 在“请求”中切换 HTTP / SQL 栏位，分别新建对应类型的请求。配置好后发送，底部查看 JSON、原文、Headers、断言、日志和执行记录。
2. 在“环境与凭据”中新建环境并编辑 JSON 对象；使用 `{{变量名}}` 引用。密码和 Token 在系统凭据库中设置，以“凭据 ID”关联数据库或认证配置。
3. 在“数据库”设置 MySQL/PostgreSQL/Oracle 连接并测试。SQL 查询使用 MySQL `?` 、PostgreSQL `$1` 或 Oracle `:1` 等原生占位符，参数通过数组绑定。首版每次请求独立连接并执行，不跨节点开启事务。
4. 在“集合”添加已保存的请求和文件夹，用上下箭头排序。配置迭代轮数及 JSON 对象数组，每轮逐行执行。默认遇到错误或失败断言停止，可选择继续；最终运行仍标记失败。
5. 在“Flows”中通过工具栏或拖放添加节点。新节点自动插入顺序链；可移动、重新连线，选中连线后按 Delete 删除。节点通过右侧配置面板删除。If/Else 和循环通过“子流程”选择器编辑内部步骤。

请求栏的“＋ 目录”可按组件创建目录及子目录，支持改名和更换上级。HTTP 与 SQL 目录独立管理；请求编辑器的“所属目录”可移动请求，搜索支持目录路径。删除目录时，请求和子目录会移到上级并保留，Collection / Flow 的请求引用不受影响。目录随工作区保存；旧工作区的请求显示在对应类型的根目录。请求目录用于归类，与集合中决定脚本执行顺序的文件夹相互独立。

三种数据库使用同一个连接编辑器和配置模型：`driver`、`host`、`port`、`database`、`username`、`secretId`。`database` 在界面统一显示为“数据库 / 服务名称”；Oracle 的 `oracleMode`（`service` / `sid`）位于附加选项，其他数据库忽略此选项。默认端口分别为 PostgreSQL 5432、MySQL 3306、Oracle 1521。切换数据库类型时更新默认端口，保留自定义端口及其他连接信息。

读取旧工作区时自动补齐缺省字段，连接 ID、密码凭据引用和额外配置会保留；下次保存时使用统一字段格式。配置不完整时可保存草稿，测试连接和运行 SQL 时统一检查主机、数据库/服务名称、用户名和密码凭据引用；端口始终要求 1–65535。密码仍通过同一系统凭据库管理。

运行前自动保存并固定配置快照；运行期间工作区禁止修改。手动编辑后请点击“保存”。单请求及 Flow 的“运行设置”可选择集合上下文，继承集合变量及对应父文件夹脚本。脚本超时也在这里设置。

## 数据与流程

执行器输出统一 JSON：`status`、`data`、`meta`、`error`、`elapsedMs`。HTTP 另有 `raw`；JSON 响应自动解析，文本保持字符串，二进制原文使用 Base64。HTTP 4xx/5xx 表示成功接收响应，是否失败由脚本断言决定。Cookie 在一次运行内共享。

SQL `data` 为 `{rows, columns, affectedRows}`。空查询仍保留列信息。NULL 为 JSON null，精确小数及超过 JavaScript 安全整数范围的整数为字符串；日期时间使用 ISO 文本，二进制使用 Base64。重复列名报错，需用 AS 指定唯一名称。不支持的列类型明确报错，可在 SQL 中转换为 text/char；首版不自动映射 PostgreSQL 数组、自定义类型等扩展类型。

参数数组支持普通字符串、布尔值、数字、null；需要精确值、JSON 或明确 NULL 类型时使用：

```json
[
  { "type": "i64", "value": null },
  { "type": "decimal", "value": "9007199254740992.1234567890" },
  { "type": "json", "value": { "enabled": true } }
]
```

可用类型：`string`、`i64`、`f64`、`bool`、`decimal`、`json`、`bytes`（Base64）。SQL 文本不进行变量拼接，动态参数使用参数数组或 FQL `bindings.params`。

Flows 按顺序执行，控制器内部为结构化子流程；禁止跨子流程连线、任意回边、并行和不连通节点。主流程必须从 Start 到 End。支持 If/Else、固定次数、While、ForEach 及嵌套。ForEach 保留当前 `item` 和 `index`，按原顺序收集循环体的最终输出；空数组输出 `[]`。Evaluate 的输出成为下一步 `input`，也可写入指定局部变量。While 可通过 Evaluate 修改局部变量推进条件。

FQL 根上下文：`input`、`variables`、`nodes`、`item`、`index`、`iteration`。`nodes` 保存最近一次节点结果信封，带连字符的节点 ID 使用 `$lookup(nodes, '完整ID').data` 访问。

```text
$map(input.rows, fn($row){ $row.id })
input.rows[enabled = true]
{"count": $count(input), "items": input}
```

请求的“脚本与输入映射”可为请求顶层配置字段指定 FQL 表达式。例如 SQL 的映射 `{"params":"[item.id]"}` 会安全绑定当前行 ID。HTTP 同样可映射 `url`、`query`、`headers` 和 `body`；字段结构需符合编辑器模型。

建议验收流程：SQL `SELECT id ...` → ForEach `input.rows` → HTTP → If `input.active = true` → SQL `UPDATE ... WHERE id = $1`（映射 `[item.id]`）→ 主流程 Evaluate `{"processed":$count(input)}`。

## 兼容边界

### 控制台与真实控制器验证

HTTP 和 SQL 的前置/后置脚本支持 `console.log()`、`console.info()`、`console.warn()`、`console.error()` 和 `console.debug()`。运行结果的“控制台”标签实时展示整个运行的输出，可筛选级别并跟随输出；每条包含时间、节点、前置/后置阶段和嵌套循环索引。日志同时输出到桌面运行进程的标准错误流，失败前的内容也会保留。运行快照保留最近 2000 条控制台记录，结束后查看 End 节点也能看到之前请求和 SQL 的输出。

```javascript
// HTTP 后置脚本
console.log("HTTP 标签", pm.response.json().tags);

// SQL 后置脚本；json() 返回 { rows, columns, affectedRows }
console.log("SQL 结果", pm.response.json().rows);
```

`postgres_tags_controllers_and_console` 集成测试使用真实 `http://localhost:9000/api/tags` 和 PostgreSQL 标签源表，验证固定循环 → While → ForEach → If/Else → 参数化 SQL → FQL 比对；全程只读取业务表。对于 3 个标签，正常场景固定循环 2 次、While 总计 4 次、ForEach 比对 12 次，条件两条分支分别执行 4 次和 8 次。还验证故意不匹配时断言失败且日志保留、零次循环、空 ForEach、While 超限及非布尔条件拒绝。

先按下方数据库测试说明设置 `BOOSTMAN_TEST_POSTGRES`，再运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml postgres_tags_controllers_and_console -- --ignored --nocapture
```

报告输出到 `artifacts/tags-controller-validation.json`（含流程配置、事件和所有场景结果），正常场景日志输出到 `artifacts/tags-controller-console.log`。二者仅用于本地验证，不提交数据库凭据或测试数据到仓库。CI 默认跳过此测试，避免连接本地业务服务。

### FQL 与脚本

FQL 是由 Rust 实现的常用子集，不是 JSONata 或 JavaScript 回退。支持字段/索引访问、过滤、数组/对象构造、fn 回调、比较、布尔、算术、三元条件，以及 `$map`、`$filter`、`$count`、`$sum`、`$exists`、`$keys`、`$lookup`、`$join`、`$split`、`$uppercase`、`$lowercase`、`$trim`、`$json`、`$jsonParse`。条件严格返回布尔值；不支持的语法、函数和参数类型明确报错。[Postman FQL 参考](https://learning.postman.com/flows/reference/flows-query-language/introduction-to-fql)。

JavaScript 在 Rust 嵌入的 QuickJS 中运行。每个请求依次执行集合、外层到内层父文件夹、请求的前置脚本，发送后再按同样顺序执行后置脚本。每个阶段中的脚本顺序等待异步任务完成。

| API                                        | 首版支持                                                                                              |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `pm.variables`                             | get/has/set/unset/clear/toObject/replaceIn；读取时按局部→迭代→环境→集合覆盖                           |
| `pm.environment`、`pm.collectionVariables` | get/has/set/unset/clear/toObject；显式修改在运行后保存                                                |
| `pm.iterationData`                         | get/has/toObject，只读                                                                                |
| `pm.request`                               | method、url.update()/字符串赋值、headers get/has/add/upsert/remove/each/all、raw body 修改            |
| `pm.response`                              | code/status/responseTime、headers、json()/text()、to.have.status()                                    |
| `pm.test`                                  | 同步/异步断言与失败汇总                                                                               |
| `pm.expect`                                | equal/eql/deep.equal/include/property/a/an/lengthOf/not/ok/true/false/null/undefined 及常用链式连接词 |
| `pm.sendRequest`                           | URL 或基本请求对象，支持回调和 Promise；使用同一 Rust HTTP 执行器                                     |
| `pm.info`、`console`                       | 迭代索引/请求 ID/名称；log/info/warn/error/debug                                                      |

未实现外部包、Node.js API、执行跳转及完整 Postman Sandbox；`pm.sendRequest` 不支持 Postman SDK 的复杂 URL 对象、文件表单对象等全部输入形态。附加请求在脚本阶段顺序执行；首版没有并行调度。未知 `pm` API 明确报错。[Postman Sandbox 参考](https://learning.postman.com/latest-v-12/docs/tests-and-scripts/write-scripts/postman-sandbox-reference/overview)。

默认请求 30 秒、脚本 5 秒、循环上限 1000 次；可配置。流程最多嵌套 32 层，运行最多 100000 步；HTTP 响应上限 20MB、SQL 查询上限 100000 行、脚本内存上限 32MB、FQL 最多 512 个词法单元。取消停止调度并释放正在使用的请求/连接，已提交的写入不会回滚。

工作区在 Tauri 应用数据目录的 `workspace.sqlite` 中；应用标识为 `com.keanu.boostman`。运行快照在本次进程内保留，最多 50 次运行，事件仅保留最近 2000 条，通过递增 sequence 与最终快照恢复漏掉的事件；重启后不恢复运行任务。环境变量、请求体和脚本是工作区明文内容；秘密应通过专用凭据引用配置。脚本本身可显式写入环境变量。

## 测试与数据库验收

### Oracle

在“数据库”中选择 Oracle，默认端口为 1521；连接方式支持 Service Name 和 SID，“数据库名称”字段随之显示相应名称。Oracle 密码仍使用系统凭据引用，SQL 调试、Collection 和 Flows 共用同一执行器。

Oracle 使用 Rust `oracle` 驱动，需要本机安装与应用架构一致的 **Oracle Instant Client Basic**，建议 19c 或更新版本。Windows 将其目录加入 PATH 后重启 Boostman；macOS/Linux 按 Oracle 客户端的动态库加载规则配置。客户端动态加载，缺少客户端时会在 Oracle 连接测试中说明原因。[驱动安装说明](https://github.com/kubo/rust-oracle#installation)。

- 连接测试使用 `SELECT 1 FROM dual`。SQL 使用 `:1`、`:2` 等连续编号占位符，与参数数组一一对应；重复和乱序引用同一编号也按编号绑定。SQL 末尾无需分号。
- 支持 SELECT、INSERT、UPDATE、DELETE、MERGE 和 DDL；DML 成功后独立提交。不支持 PL/SQL、OUT 参数、RETURNING INTO 或跨节点事务。Oracle DDL 会隐式提交，取消无法撤销已提交的操作。
- Oracle 默认将未加双引号的列名转为大写；FQL 中需按原始大小写访问，或使用 `AS "id"` 等双引号别名统一字段名。
- NUMBER 的精确小数和超出安全整数范围的整数以字符串输出；安全整数输出 JSON 数字。DATE/TIMESTAMP 输出 ISO 文本，TIMESTAMP WITH TIME ZONE 保留偏移；RAW/BLOB 为 Base64，CLOB/NCLOB 为字符串。原生 JSON 列请用 `JSON_SERIALIZE(... RETURNING CLOB)` 显式序列化，应用端可再用 `$jsonParse` 处理。
- 支持现有参数类型；Oracle 的 `bool` 参数映射为数字 0/1，`json` 参数为 JSON 文本 CLOB，适用于支持 JSON 文本存储的列。Oracle 将空字符串按 NULL 处理。
- Oracle 阻塞调用在独立工作线程中执行，取消通过连接中断传递给 OCI，并等待执行线程结束；连接使用网络超时，SQL 使用 OCI 调用超时。取消或超时的返回可能包含 Oracle 的网络清理时间，不会把未停止的 SQL 留给后续节点继续调度。

没有 Oracle 服务时，本地单元测试仍可验证 Service Name/SID 描述符、编号参数映射、精确数值、日期时区和提前取消；真实连接、CRUD 与 Flows 集成测试需单独运行：

```powershell
$env:BOOSTMAN_TEST_ORACLE = '{"host":"localhost","port":1521,"database":"FREEPDB1","oracleMode":"service","username":"boostman_test","password":"your-test-password"}'
cargo test --manifest-path src-tauri/Cargo.toml oracle_end_to_end -- --ignored
```

该集成测试会创建并清理独立测试表和临时凭据，验证连接、绑定、增删改查、小数和大整数精度、空结果、重复列名、日期/二进制，以及 SQL → ForEach → SQL 更新 → FQL 汇总。使用独立测试账号，需要建表和操作自身测试表的权限。

默认测试不依赖外部服务，覆盖 Rust HTTP 本地服务、FQL、QuickJS、工作区重新打开、嵌套控制流、取消与三个执行入口的一致性。

真实数据库端到端测试默认忽略，需要专用测试库和建表/写入/删除测试表权限。测试创建 UUID 命名的表和临时系统凭据，在结束时清理。它们验证参数绑定、精确小数、NULL、空结果列信息、重复列名、真实查询超时，以及“SQL → ForEach → HTTP → If → SQL 更新 → 汇总 JSON”闭环。

```powershell
$env:BOOSTMAN_TEST_POSTGRES = '{"host":"localhost","port":5432,"database":"boostman_test","username":"postgres","password":"your-test-password"}'
cargo test --manifest-path src-tauri/Cargo.toml postgres_end_to_end -- --ignored
$env:BOOSTMAN_TEST_MYSQL = '{"host":"localhost","port":3306,"database":"boostman_test","username":"root","password":"your-test-password"}'
cargo test --manifest-path src-tauri/Cargo.toml mysql_end_to_end -- --ignored
```

首版不包含 Postman 资产导入、云协作、跨节点事务、并行分支、客户端证书、自定义代理或 OAuth 2.0 授权流程。HTTP 客户端使用系统可用的代理环境配置。
