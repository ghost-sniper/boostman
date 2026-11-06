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
| Windows | `x86_64-pc-windows-msvc`   | MSI                | `boostman-windows-x86_64-msi` |
| Windows | `x86_64-pc-windows-msvc`   | NSIS 安装 EXE      | `boostman-windows-x86_64-exe` |
| Linux   | `x86_64-unknown-linux-gnu` | deb                | `boostman-linux-x86_64-deb` |
| Linux   | `x86_64-unknown-linux-gnu` | rpm                | `boostman-linux-x86_64-rpm` |
| macOS   | `x86_64-apple-darwin`      | DMG                | `boostman-macos-x86_64-dmg` |

在仓库 Actions 的对应运行页面按格式分别下载 Artifact，默认保留 14 天。每个产物仅包含一种安装包及对应的 `SHA256SUMS.txt`，解压后可在同一目录核对校验值。上传前检查所需包格式、程序的实际 CPU 架构，以及 deb/rpm 的架构元信息；不生成 ARM 或 Universal 包。macOS 明确使用 Intel runner，包采用 ad-hoc 签名，尚未配置 Apple 开发者证书或公证；Windows 安装包也未配置发布者签名。工作流不会自动创建 GitHub Release。

CI 不连接业务数据库，也不包含数据库密码；需外部服务的集成测试保持默认忽略。Oracle Instant Client 仍由使用 Oracle 功能的目标机器提供，不随安装包分发。

## 使用

1. 在“请求”中切换 HTTP / SQL 栏位，分别新建对应类型的请求。配置好后发送，底部查看 JSON、原文、Headers、断言、日志和执行记录。
2. 在“环境与凭据”中新建环境并编辑 JSON 对象；使用 `{{变量名}}` 引用。密码和 Token 在系统凭据库中设置，以“凭据 ID”关联数据库或认证配置。
3. 在“数据库”设置 MySQL/PostgreSQL/Oracle 连接并测试。SQL 查询使用 MySQL `?` 、PostgreSQL `$1` 或 Oracle `:1` 等原生占位符，参数通过数组绑定。首版每次请求独立连接并执行，不跨节点开启事务。
4. 在“集合”添加已保存的请求和文件夹，用上下箭头排序。配置迭代轮数及 JSON 对象数组，每轮逐行执行。默认遇到错误或失败断言停止，可选择继续；最终运行仍标记失败。
5. 新建 Flows 使用端口数据流画布：底部“＋ Block”打开组件菜单，卡片内直接编辑参数，从输出端口拖到输入端口连线。卡片齿轮打开详细设置。旧流程保留兼容模式，仍按原有顺序链和子流程执行。

请求栏的“＋ 目录”可按组件创建目录及子目录，支持改名和更换上级。HTTP 与 SQL 目录独立管理；请求编辑器的“所属目录”可移动请求，搜索支持目录路径。删除目录时，请求和子目录会移到上级并保留，Collection / Flow 的请求引用不受影响。目录随工作区保存；旧工作区的请求显示在对应类型的根目录。请求目录用于归类，与集合中决定脚本执行顺序的文件夹相互独立。

HTTP 栏的“导入 cURL”支持粘贴单条命令，并在当前目录新建请求。支持 POSIX 单/双引号、反斜杠续行、`-X`、`-H`、`-b`、`-u`、`--oauth2-bearer`、`-L`、`-I`、`-G`、`-m`、`--json`、`--data*`、`--data-urlencode`、`-F` 和 `--form-string`。文件内容仅支持 `--data-binary @路径` 和 multipart 文件字段；导入不执行命令、不读取文件、不发送请求。Basic/Bearer 认证参数写入系统凭据库，工作区保存凭据引用。暂不支持 shell 变量展开、`$'…'` 字符串、管道、重定向、多个 URL、Cookie 文件及 multipart 修饰符，未知选项会报错。

HTTP 请求标题栏的“导出请求”提供 cURL 命令、PowerShell 脚本和 Bash 脚本，可预览、复制或下载。PowerShell 使用 `Invoke-WebRequest`（需要 PowerShell 7.0+），Bash 使用 **GNU Wget**（不支持 BusyBox wget），cURL 格式仍使用 curl；脚本包含方法、URL、启用的参数/Headers、认证占位、请求体、超时和重定向设置。两种脚本均支持 JSON/文本、URL 编码表单、文件和 multipart；PowerShell 使用 HttpContent 处理 multipart，Bash 使用临时文件构造 multipart 并在退出时清理。超时和重定向遵循工具自身行为：PowerShell 超时向上取整为秒，Wget 的超时为网络空闲超时；超过重定向上限会使脚本失败。参考 [Invoke-WebRequest 文档](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/invoke-webrequest?view=powershell-7)和 [GNU Wget 手册](https://www.gnu.org/software/wget/manual/wget.html)。

导出当前静态配置，不运行前后置脚本、不解析环境变量；系统凭据以 `{{secret:凭据ID}}` 占位，运行前需替换。Headers/请求体中的明文原样导出。请求仍通过 Rust 引擎执行，导入导出不新增执行入口。

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

新建 Flows 保存 `executionMode: "dataflow"`，执行由真实端口连线驱动：输出可连接多个组件，每个输入最多一条线，多个已连接输入全部到达后执行。If/Condition/Validate 只向匹配端口发送数据，未选路径标记跳过；OR 从两个输入中选择先到达的数据，不等待另一路，保留 false/null。调度器按确定性顺序执行，不进行网络请求的并行竞速；禁止任意回边，重复执行使用循环组件。数据常量及 Start 在运行开始时提供数据，Start 卡片可以配置运行输入。

旧流程没有 executionMode 字段，按兼容模式执行：控制器内部为结构化子流程，主流程必须从 Start 到 End。旧数据与节点 ID 不自动改写，避免把顺序链误当成端口数据流。

端口模式下，Repeat 的 Count、ForEach 的 List 输入支持连接上游；可选 Start 输入用于控制启动时机。循环体接 Repeat / Item 输出，每轮传递 `item` 和 `index`；Collect 的 Item 接收每轮数据，结束后从 List 输出完整有序数组，从 Finish 输出完成事件；零次循环也输出 `[]`，嵌套循环隔离收集。Evaluate 使用 FQL，输入变量可连接其他组件或设置 FQL 默认值。端口旁路径字段使用 JSON Pointer（如 `/body/rows`）提取上游数据，不存在时明确报错。

FQL 根上下文：`input`、`variables`、`nodes`、`item`、`index`、`iteration`。`nodes` 保存最近一次节点结果信封，带连字符的节点 ID 使用 `$lookup(nodes, '完整ID').data` 访问。

Flows 还提供以下数据与展示组件，均由 Rust 执行，可放入分支及循环体：

| 组件 | 配置与行为 |
| --- | --- |
| 常量 | 任意 JSON 值，支持字符串、数字、布尔值、null、数组和对象。 |
| List | 按配置顺序求值每项 FQL，输出 JSON 数组；空配置输出 `[]`。 |
| Record | 每个字段配置名称与 FQL，输出 JSON 对象；重复或空字段名报错。List/Record 各项读取相同的节点输入，可引用已有节点结果，每个组件最多 10000 项。 |
| Select | 使用 JSON Pointer 取值，例如 `/rows/0/id`；空路径选择全部，`~1` 表示 `/`，`~0` 表示 `~`；缺失路径报错，实际 null 保留。 |
| Delay | FQL 返回 0–600000 的整数毫秒数，等待期间支持取消，结束后透传原始输入。 |
| Log | 求值 FQL 并实时写入控制台及最终快照，支持 log/info/warn/error/debug；保留节点及循环路径，透传原始输入。 |
| Display | FQL 选择待展示数据，支持 JSON、文本、表格、布尔值、折线图；严格检查数据类型，透传原始输入。 |

Display 会在画布、节点配置面板及执行结果的“组件展示”中呈现。表格要求对象数组；折线图要求数值数组或包含自定义横轴字段及数值纵轴字段的对象数组。循环中保留最后一次展示，历史各次输出可从执行记录查看；最终上下文可恢复事件缓冲区中已移除的展示结果。界面预览限制表格 200 行 / 32 列、折线图 500 个点（画布缩略预览更少），不截断执行数据。

组件语义参考 Postman 的 [List](https://learning.postman.com/flows/reference/blocks/list)、[Record](https://learning.postman.com/flows/reference/blocks/record/)、[Select](https://learning.postman.com/flows/reference/blocks/select/)、[Delay](https://learning.postman.com/flows/reference/blocks/delay/) 和 [Display](https://learning.postman.com/flows/reference/blocks/output-display/) 文档。端口模式的 List/Record 可逐项连接数据，未连接项使用内联 FQL；兼容模式沿用 FQL 引用和结构化子流程。

画布卡片使用紧凑灰色标题栏、内联输入、端口标签、输入来源、可选路径、可展开输出预览和注释；Start 带橙色标记，Display 在卡片内呈现数据。Create/Get Variable 可按创建组件引用，重命名同步到引用方，并用相同颜色和选择时的虚线标明关系。请求仍复用 Rust HTTP/SQL 执行器：HTTP Success/Fail 输出包含 `body`、`headers`、`status`、`code`、`responseTime`，传输错误可沿已连接的 Fail 端口处理；测试结果从 Tests 输出。

组件工具栏支持按请求、逻辑与循环、数据、日期与时间、变量、输出与展示分类。以下组件同样由 Rust 执行，并随工作区保存：

| 组件 | 行为与兼容范围 |
| --- | --- |
| Condition | 顺序求值最多 100 个 FQL 布尔条件，只进入首个匹配的子流程；无匹配时进入 Default。分支有稳定 ID，调整顺序不改变引用；删除分支会同时删除该分支及其嵌套节点。 |
| Validate | 用 JSON Schema 校验 input，进入 Pass / Fail 子流程；不符合 Schema 属于 Fail 分支而非执行错误，Schema 本身无效会失败。支持内嵌 `$ref`，禁止读取外部 URL/文件；校验详情最多保留 100 条，可在节点配置及运行汇总查看。 |
| Evaluate / Repeat | 延续现有 FQL 求值和固定次数结构化循环，补齐英文组件名称；Evaluate 不执行 TypeScript。 |
| OR | 结构化分支之后的合流节点，透传已执行分支的结果，保留 false/null；不是布尔运算，也不模拟并行输入竞争。 |
| Collect | 放在循环体内时收集各轮表达式结果，完成后将完整数组写入该 Collect 节点结果；空循环产生 `[]`，嵌套循环每次调用独立收集。循环结束后可用 `$lookup(nodes, 'Collect节点ID').data` 引用，最多 10000 项。也可放在循环之后直接接收数组。 |
| String / Bool / Number / Null | 明确类型的 JSON 常量；不自动将字符串转成数字或布尔值。 |
| Now / Date / Date & Time | 输出 Unix 毫秒时间戳；Now 取执行时刻，Date 使用所选日期的 UTC 零点，Date & Time 接受含 Z 或时区偏移的 ISO 8601 时间，拒绝无时区或无效日期。 |
| Create Variable / Get Variable | 创建或更新运行局部变量，Get 读取已经创建的变量，保留实际 null，未创建时报错；不跨运行持久化。后续 FQL 也可通过 variables 访问。 |
| Output / Flow | 一个流程最多有一个 Output。端口模式可通过 Flow 组件调用其他包含 Output 的已保存端口流程，将输入传入 Start，接收 Output 返回值；局部变量隔离，共用运行快照中的环境、连接、请求，拒绝递归及超过 32 层调用。兼容模式 Output 仍位于主流程。 |
| Response | 配置状态码、字符串 Headers 及 FQL Body，输出本地 `{statusCode, headers, body}` 并存入快照 response；一条执行路径只执行一个 Response。此组件不启动 HTTP 监听、不提供云端请求 URL。 |

结果区域的“流程输出”展示 Output / Response；其他执行细节保留在运行汇总。Condition/Validate 的未选分支及嵌套节点会标记跳过。相关 Postman 文档：[Condition](https://learning.postman.com/flows/latest-v-12/reference/blocks/condition)、[Validate](https://learning.postman.com/flows/reference/blocks/validate/)、[OR](https://learning.postman.com/flows/reference/blocks/or/)、[Collect](https://learning.postman.com/flows/reference/blocks/collect/)、[Response](https://learning.postman.com/flows/reference/blocks/response/)、[Output](https://learning.postman.com/flows/reference/blocks/output-module/)。

上表涉及“子流程”和顺序合流的条目描述旧版兼容模式；新建端口模式以端口说明为准。本地组件已经实现端口数据传递，但不宣称与整个 Postman 产品完全一致：Evaluate 尚不支持 TypeScript；Response 仍为本地响应结果，不提供 Request 触发器、云端 URL 或 HTTP 监听；不包含云部署、AI Evals、外部连接器和并行竞速。

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
