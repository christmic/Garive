# Garive macOS 用户手册

> **草案，不可发布。** 本手册的候选版本、包校验和与 62 项运行截图尚未完成。
> 只有 `node desktop/release/verify-desktop-evidence.mjs` 通过后，才可移除本声明。

| 项目 | 当前值 |
|---|---|
| 手册版本 | Draft 1 |
| 应用版本 | 0.1.0 |
| 候选 Git revision | 待录入 |
| 候选安装包 SHA-256 | 待录入 |
| 截图 manifest SHA-256 | 待录入 |
| 已验证 macOS | 待录入 |

本手册面向使用 Garive 完成本地持久工作的 macOS 用户。Garive 将会话、轮次、
活动和交付物保存在本地 Runtime 中；文件夹访问由 macOS 原生授权控制。界面不会
接收提供商凭据、书签字节或文件系统路径。

## 1. 安装、验证与首次启动

### 1.1 安装候选版本

起点：Finder 中已经下载了发布页列出的 Garive DMG。

1. 对照发布页计算 DMG 的 SHA-256；数值必须与本手册首页一致。
2. 打开 DMG，将 Garive 拖入 Applications。
3. 从 Applications 启动 Garive。不要绕过 Gatekeeper，也不要运行来源不明的副本。

预期结果：macOS 接受 Developer ID 签名与公证票据，Garive 显示首次设置窗口。

恢复：校验和、签名或 Gatekeeper 任一失败时停止安装，保留原文件用于核对发布来源。

<!-- SCREENSHOT M01 PENDING: DMG and Applications handoff -->
<!-- SCREENSHOT M81 PENDING: clean-Mac Gatekeeper launch -->

### 1.2 连接本地 Runtime

起点：首次启动的 Connect 页面。

1. 选择 Runtime preset 与 connection profile。
2. 填写 model target、model ID、Agent definition；只有需要时才展开 endpoint override。
3. 在 Credential 安全字段中输入凭据，选择 **Review setup**。
4. 核对 Review 页的 provider、model 与 Agent 摘要。该页不得显示 credential。
5. 选择 **Commit configuration**，随后选择 **Restart Garive**。

预期结果：凭据仅写入操作系统安全存储；不可变配置提交后，应用明确重启并进入 Work。

恢复：输入错误时返回 Connect 修正；提交失败时 credential 字段会清空，重新输入即可。
遇到 stale/conflict 时重新生成 Review，不要重复使用旧计划。

<!-- SCREENSHOT M02 PENDING: first launch Connect -->
<!-- SCREENSHOT M03 PENDING: redacted Review -->
<!-- SCREENSHOT M04 PENDING: write-only credential action -->
<!-- SCREENSHOT M05 PENDING: restart required -->
<!-- SCREENSHOT M06 PENDING: returning launch -->

## 2. 认识主窗口和状态

主窗口由五部分组成：左侧导航与 Recents、中央对话时间线、底部 Composer、可折叠
Inspector，以及显示本地 Runtime 状态的标题区域。

- **Local / 已就绪**：本地 Runtime 可以接受新工作。
- **Working / 工作中**：当前轮次已提交，尚未进入终态。
- **Needs input / 需要输入**：同一轮次等待文本或受治理决定。
- **Completed / 已完成**：结果已经由本地 Runtime 提交。
- **Verified preview / 已验证预览**：Artifact 字节已与持久摘要匹配。
- **Unavailable / 不可用**：能力未安装或恢复条件尚未满足；不是后台仍在偷偷执行。

状态文字是权威提示，动画本身从不代表提交或成功。

## 3. 创建、推进与恢复工作

### 3.1 创建第一项工作

起点：空白 Work 页面。

1. 选择一个 outcome 建议，或直接在 Composer 描述最终成果。
2. `Return` 发送；`Shift-Return` 插入换行。中文输入法候选确认不会发送。
3. 查看时间线与 Activity 的已提交状态。

预期结果：建议只填入 Composer 并保留焦点；发送后创建持久 Session 和 Turn，完成结果
安全渲染 Markdown、表格与任务列表。

恢复：提交失败时草稿和当前页面保留；按照错误行的下一步重试，而不是重建 Session。

<!-- SCREENSHOT M10 PENDING: outcome-first home -->
<!-- SCREENSHOT M11 PENDING: draft and context area -->
<!-- SCREENSHOT M12 PENDING: committed running state -->
<!-- SCREENSHOT M13 PENDING: completed GFM result -->

### 3.2 在同一 Session 继续

起点：已有一个完成结果的时间线。

1. 在 Composer 输入补充要求并发送。
2. 如果界面显示文本 suspension，在 continuation Composer 提供所需输入。
3. 保持原窗口，等待同一 Turn 从持久 cursor 继续。

预期结果：第二轮保留之前结果；suspension 被精确恢复，不会偷偷创建替代 Turn。

恢复：重启应用后从 Recents 打开同一 Session；时间线应保持顺序并包含后续分页内容。

<!-- SCREENSHOT M14 PENDING: second Turn -->
<!-- SCREENSHOT M15 PENDING: exact input suspension -->
<!-- SCREENSHOT M16 PENDING: restart-restored timeline -->

### 3.3 复制与导出回答

完成的普通回答可用 **Copy** 复制，或用 **Export Markdown** 导出 `.md`。这与 Artifact
导出不同：普通回答是公开响应快照，受治理的工作区文件仍需走 Artifact 流程。

## 4. 查找持久工作

### 4.1 Recents 与 Search

起点：已有多个持久 Session。

1. 在 Recents 单击项目即可打开。
2. 按 `Command-K`、`Command-F`，或从 File 菜单选择 **Search Work…**。
3. 输入首个请求中的关键词；选择结果返回对应 Session。

预期结果：搜索只读取最近本地 Session，不创建云端索引；结果显示正确 Turn 数量。

恢复：无匹配时查询仍可编辑。清空或更换关键词；空状态不会暗示网络搜索。

<!-- SCREENSHOT M20 PENDING: durable Recents -->
<!-- SCREENSHOT M21 PENDING: focused Search -->
<!-- SCREENSHOT M22 PENDING: matching result -->
<!-- SCREENSHOT M23 PENDING: no match -->

### 4.2 新建工作

按 `Command-N` 或选择 File → **New Work**。这会清理当前临时 context 并聚焦 Composer，
不会删除历史 Session。

<!-- SCREENSHOT M24 PENDING: clean New Work -->

## 5. 使用本地 Workspace

### 5.1 添加只读上下文

起点：Composer 下方没有选中文件。

1. 选择 **Add context**，在 macOS 原生面板选择一个具体文件夹。
2. 在 Garive 文件选择器中进入子目录；使用 breadcrumb 返回，必要时 **Load more**。
3. 选择最多八个受支持的 UTF-8 文本文件，然后选择 **Add**。
4. 发送前可从 chip 移除错误文件。

预期结果：React 只看到安全元数据；文件正文从 Rust 后端直接进入 Runtime。根目录、Home、
symlink root、隐藏文件与受保护 package 按策略拒绝。

恢复：原生面板 Cancel 不改变状态；读取失败可 Retry 或选择另一个 Workspace。文件选择器
支持 Tab/Shift-Tab 循环、Escape 关闭并恢复 Composer 焦点。

<!-- SCREENSHOT M30 PENDING: native folder picker -->
<!-- SCREENSHOT M31 PENDING: safe in-app picker -->
<!-- SCREENSHOT M32 PENDING: nested pagination -->
<!-- SCREENSHOT M33 PENDING: pre-send chips -->
<!-- SCREENSHOT M34 PENDING: committed read-only attachment -->

### 5.2 授权输出

1. 选择 **Allow outputs**。
2. 在原生面板重新选择同一个 Workspace 根目录。
3. 确认 Composer 显示 Output folder enabled。

预期结果：write grant 仅在当前进程中有效；持久恢复仍为只读。不同根目录不会继承原身份。

<!-- SCREENSHOT M35 PENDING: write authorization and badge -->

### 5.3 Detach、恢复和撤销

- **Detach** 只从当前 Session 移除 Workspace，并提交持久 receipt；不会全局撤销授权。
- Settings → Workspace access 中的 **Restore access** 要求重新选择原文件夹；界面不显示路径。
- **Remove access** 第一次仅展示后果；第二次确认才立即撤销后续读取和输出。

预期结果：操作完成后重新加载后端真相，焦点回到 Composer 或阻塞中的安全批准按钮。既有
receipt 保留；Keychain 清理失败会在重启后有限重试。

恢复：选择错误文件夹时改选原文件夹；撤销失败时权限不会被扩大，也不要假设已经成功。

<!-- SCREENSHOT M36 PENDING: detached Workspace -->
<!-- SCREENSHOT M37 PENDING: dormant recovery row -->
<!-- SCREENSHOT M38 PENDING: two-step revoke -->
<!-- SCREENSHOT M39 PENDING: cleanup retry status -->

## 6. 批准、Activity 与故障恢复

### 6.1 处理精确写入批准

批准卡必须同时显示 operation、Workspace、one-file scope、当前 prepared call 的一次性时效，
以及 **Never overwrite**。键盘进入卡片时，**Decline** 默认获得焦点。

1. 不认可时选择 **Decline**；该决定会持久化，文件不会创建。
2. 认可时选择 **Approve once**；修改请求、grant 或目标后必须重新批准。
3. 在 Activity 查看 prepared → authorized → running → completed 的已提交顺序。

<!-- SCREENSHOT M40 PENDING: exact approval card -->
<!-- SCREENSHOT M41 PENDING: safe Decline focus -->
<!-- SCREENSHOT M42 PENDING: durable denial -->
<!-- SCREENSHOT M43 PENDING: committed approval sequence -->

### 6.2 故障与离线

- Runtime/provider 失败：保留当前位置和草稿，按具体错误重试。
- Projection unavailable：结果可能已提交；只重读投影，不要重复执行工作。
- 网络离线：界面不得显示假进度；恢复连接后从持久 cursor 继续，不重复已有事件。

<!-- SCREENSHOT M44 PENDING: inline Runtime recovery -->
<!-- SCREENSHOT M45 PENDING: projection recovery -->
<!-- SCREENSHOT M46 PENDING: offline and reconnect -->

## 7. 检查与导出 Artifact

### 7.1 验证预览

1. 打开 Inspector，选择 **Artifacts**。
2. 选择已提交交付物的 **Preview**。

预期结果：卡片元数据来自 receipt-bound immutable projection；预览只在重新计算 SHA-256
与提交摘要一致后显示，内容有大小边界且不会被 live region 整段朗读。

恢复： backing bytes 被修改或权限不可用时预览 fail closed，持久元数据仍保留。

<!-- SCREENSHOT M50 PENDING: committed Artifact -->
<!-- SCREENSHOT M51 PENDING: verified preview -->
<!-- SCREENSHOT M52 PENDING: changed backing bytes -->

### 7.2 导出副本

1. 选择 **Export copy…**，在 macOS save panel 中确定新文件名和位置。
2. 确认成功状态，并在 Finder 或校验工具中核对字节与 SHA-256。

预期结果：目的路径不会进入 React；一次性 capability 在尝试后消耗，Garive 原子创建新文件，
绝不覆盖现有目标。

恢复：目标存在时换名；Cancel 不创建文件。崩溃后，下次显式授权该目录时只清理精确记录的
临时文件，不影响其他内容。

<!-- SCREENSHOT M53 PENDING: native save panel -->
<!-- SCREENSHOT M54 PENDING: successful exact export -->
<!-- SCREENSHOT M55 PENDING: no-overwrite error -->
<!-- SCREENSHOT M56 PENDING: crash cleanup -->

## 8. Inspector、Settings 和 macOS 菜单

Inspector 的 Activity/Artifacts 使用标准 tab semantics。`Command-Shift-A` 与 View →
**Toggle Inspector** 等价；窄窗口中 Inspector 成为有界 overlay，不造成时间线横向滚动。

Settings 提供：

- Appearance：System / Light / Dark；
- Density：Comfortable / Compact；
- Language：System / English / 简体中文；
- Runtime：后端报告的真实能力；
- Workspace access：无路径的恢复与撤销；
- Privacy：哪些信息不会进入前端。

View 菜单使用 `Command-=` 放大、`Command--` 缩小、`Command-0` 恢复实际大小。缩放档位为
80%、100%、120%、150%、175%、200%。语言变化会同步重建完整 Garive/File/Edit/View/
Window 菜单，不改变快捷键。

<!-- SCREENSHOT M60 PENDING: Activity tab -->
<!-- SCREENSHOT M61 PENDING: Artifacts tab -->
<!-- SCREENSHOT M62 PENDING: Appearance settings -->
<!-- SCREENSHOT M63 PENDING: Runtime truth -->
<!-- SCREENSHOT M64 PENDING: localized native menu bar -->
<!-- SCREENSHOT M65 PENDING: compact/fullscreen layout -->
<!-- SCREENSHOT M66 PENDING: quit and restored Session -->

## 9. 键盘、VoiceOver、缩放与中文

主要快捷键：

| 操作 | 快捷键 |
|---|---|
| New Work | `Command-N` |
| Search | `Command-F` 或 `Command-K` |
| Toggle Inspector | `Command-Shift-A` |
| Settings | `Command-,` |
| Zoom In / Out / Actual Size | `Command-=` / `Command--` / `Command-0` |

所有流程必须可只用键盘完成；modal 包含并恢复焦点。VoiceOver 应读出 landmark、按钮名称、
tab 状态、有限 live status 和不含私密路径的批准摘要。200% 下主要操作仍可见，Settings 可
纵向滚动，时间线不出现横向滚动。Increase Contrast、Reduce Transparency、Reduce Motion
不应抹去焦点、边界、错误或状态差异。

中文输入法组合态确认候选不会发送 Composer；简体中文和 QA pseudolocale 不允许出现原始
message key、文字截断或不可访问控件。用户文本、model 输出、文件名、Workspace 名称、
Agent ID 与 receipt 事实保持原样，不被翻译。

<!-- SCREENSHOT M70 PENDING: keyboard-only focus journey -->
<!-- SCREENSHOT M71 PENDING: VoiceOver rotor and approval -->
<!-- SCREENSHOT M72 PENDING: native 200% Work/approval/Artifact/Settings -->
<!-- SCREENSHOT M73 PENDING: dark semantic parity -->
<!-- SCREENSHOT M74 PENDING: contrast/transparency/motion preferences -->
<!-- SCREENSHOT M75 PENDING: full Simplified Chinese journey -->
<!-- SCREENSHOT M76 PENDING: expanded pseudolocale -->

## 10. 更新、休眠、卸载与数据保留

About/version 必须与签名 bundle、manifest 和本手册首页一致。更新只有在签名验证后安装；
无效或降级包必须安全拒绝并保持当前版本可用。睡眠、唤醒或网络中断后，同一 Session 从
持久 cursor 继续。

在公开签名版本中，打开 **Settings → Update**：

1. 先核对 **Current version**，再选择 **Check for updates**。检查失败只影响本次检查，
   不会替换当前应用；网络恢复后可手动重试。
2. 出现更高的稳定版本时核对 **Target version**，选择 **Download update**。下载进度不会
   把“已下载”冒充“已验证”；签名失败会停止在拒绝状态。
3. 只有状态明确为已验证时才选择 **Install verified update**。正在提交 Session 时重启
   操作保持禁用，先等待提交得到持久结果或显式取消。
4. 安装完成后选择 **Restart Garive**。重启后 Current version 必须等于先前显示的 Target
   version，且已有 Session、Workspace 授权状态与 Artifact 仍可恢复。
5. 若安装开始后结果无法证明，界面显示 **Outcome unknown**。不要重复下载或安装；先重启
   Garive，让应用用持久 pending record 对照实际版本。仍无法对账时保留当前数据并联系支持。

相同版本、旧版本和预发布版本一律不会进入下载；本地无通道构建会明确显示 Update
Unavailable，也不会发起更新网络请求。

将 Garive.app 移到废纸篓只移除应用本身，不等同于删除本地 Runtime 数据、配置或 Keychain
授权。先在 Settings 撤销 Workspace access；如需完全移除数据，请仅按照与该发布版本一起
验证的卸载步骤操作。不要手工删除不明 Keychain 条目或整个 Application Support 目录。

<!-- SCREENSHOT M80 PENDING: About and build identity -->
<!-- SCREENSHOT M82 PENDING: valid update -->
<!-- SCREENSHOT M83 PENDING: invalid/downgrade refusal -->
<!-- SCREENSHOT M84 PENDING: sleep/wake and reconnect -->
<!-- SCREENSHOT M85 PENDING: uninstall/data retention explanation -->

## 11. 可以安全分享的诊断信息

可以分享：应用版本、Git/build identity、macOS 版本、机器架构、失败的稳定错误代码、能力
Available/Not installed 状态，以及不含内容的 screenshot ID。

不要分享：credential、endpoint 中的秘密、Workspace 路径、用户或 model 原文、文件内容、
bookmark bytes、数据库、Keychain 导出、原始 Runtime facts 或带私人背景的截图。

## 当前能力边界

本手册最终版只能描述候选包真实安装的能力。当前本地候选已实现更新状态机并生成 SBOM，
但仍未通过 Developer ID、公证、真实签名更新/降级、clean-Mac、VoiceOver、原生 200% 和
M01–M85 全矩阵门禁；因此本草案不可作为公开发布说明。路线图能力不得以灰色可点击控件或
“即将完成”的方式伪装成已可用功能。
