# CLO Mac · 家庭财富终端

本仓库是 CLO 的 macOS 客户端（家庭财富终端），也是这份桌面程序的独立源码。菜单栏里看今日盈亏，左键是原生概览，要看网页工作台再打开。账本、网页和 API 仍在 [compounding-life-os](https://github.com/briskkk/compounding-life-os)，不在本仓库。Linux 上不能打出可运行的 `.app`，下面的步骤需要在 Mac 上、本仓库根目录执行。

## 它做什么

- 菜单栏用系统标题显示今日盈亏，旁边留着模板图标。涨为红、跌为绿。标题不会被一张自定义位图替换掉，避免状态项缩没。悬停和右键菜单显示净资产、可投资资产和待办。
- 左键打开菜单栏上的原生概览，不打开网页。概览用简体中文显示今日盈亏（红涨绿跌）、净资产、可投资资产和待办。要看账本细节时，用概览或右键菜单里的「打开工作台」。工作台仍是 `https://www.huayanpei.com` 的网页，不是另一套页面。
- 未登录时菜单栏和概览只显示「请登录」，不显示数字。`401` 是未登录。生产上没有会话、只读 Key 也没配置时，接口会回 `503`，正文是 `PORTFOLIO_API_KEY not configured`，这时同样显示「请登录」。其它 `503` 不退出登录：还留着上一笔数字就继续显示，并标明暂时没有读到新的；没有上一笔才显示「暂无数据」。客户端不使用全局只读 Key。登录页就是网站自己的登录页，从「打开工作台」进去。
- 有待办时发系统通知。第一次看到会提醒一次；之后只在出现新待办或数量增加时再提醒。点通知会打开对应页面。
- 自动刷新看交易时段。A 股、港股或美股至少有一个在常规交易时段时，每 60 秒刷新一次；三个市场都休市（含午休）后改为 15 分钟，下一档开市如果更近会提前醒来。钟点按 Asia/Shanghai 显示。A 股和港股用这个时区；美股用纽约时间，含夏令时，不含交易所节假日。右键菜单里的「立即刷新」由人触发，不受这个间隔限制。

```
Mac 菜单栏 / 窗口
    |  HTTPS
    v
https://www.huayanpei.com/api/*
    |
    v
同一套引擎和数据库（compounding-life-os）
```

## 用哪些接口

这些接口由 [compounding-life-os](https://github.com/briskkk/compounding-life-os) 提供。本仓库只调用它们。

| 用途 | 接口 | 说明 |
| --- | --- | --- |
| 今日盈亏、净资产、可投资 | `GET /api/mobile/summary` | 生产环境已有。客户端只展示 `netWorth.dayPnl`、`dayPct`、`netWorth`、`investable` |
| 三类待办 | `GET /api/desktop/tasks` | 只读。返回总览引擎 `buildOverviewTasks` 的结果 |
| 登录 | `POST /api/auth/login` | 网页用 cookie，菜单栏用返回的 Bearer token |
| 登出 | `POST /api/auth/logout` | 网页登出时菜单栏一并清除钥匙串 |

待办对应的页面：

- 基金份额待确认 → `/fund-confirmations`
- 定投节点待核对 → `/investment-plans`
- 账户待对账 → `/reconciliation`

## 生产部署

菜单栏盈亏和「基金份额待确认」可以连当前的 https://www.huayanpei.com。定投节点和账户对账要等 `GET /api/desktop/tasks` 部署到生产后才会出现。该路由在网页仓库 [compounding-life-os](https://github.com/briskkk/compounding-life-os)。在那之前，菜单会写明这两类待办还不能从服务器读到，不会把它们显示成 0。

这个接口不写账本，也不改部署脚本。上线时在 compounding-life-os 按现有方式发布包含该路由的 Next.js 版本即可。

## 在 Mac 上运行

需要：

- macOS 12 或更新版本
- Xcode 命令行工具：`xcode-select --install`
- Rust 1.90 或更新版本：<https://rustup.rs>
- Node.js 22

在本仓库根目录执行：

```bash
npm install
npm test
npm run dev
```

`npm test` 只跑与界面无关的解析、提醒规则和刷新节奏。`npm run dev` 要在 macOS 上、本仓库根目录执行。它会构建调试包 `src-tauri/target/debug/bundle/macos/家庭财富终端.app` 并打开这个 `.app`，不会直接运行 `target/debug/family-wealth-terminal`。

已经在跑的未签名 `.app` 不会自己换成新逻辑。菜单栏左键要变成概览、刷新要跟着交易时段走，需要在本机重新构建，退出旧进程后再打开新的 `.app`。旧进程可能仍是以前写死的间隔，包括更早的 15 分钟一轮。

那个裸二进制没有 `Info.plist`，`CFBundleIdentifier` 是空的。macOS 这时只在屏幕左上角显示「家庭财富终端」应用菜单，菜单栏右侧不会出现「请登录」或当日盈亏。调试包和 `npm run build` 的发行包使用同一个 `CFBundleIdentifier`：`com.huayanpei.family-wealth`。发行包正在运行时要先退出，再执行 `npm run dev`，否则相同的 bundle id 会让新实例交给已经打开的发行包。

开发用静态页监听 `127.0.0.1:31420`。这个端口仍被占用时，换一个再启动：

```bash
CLO_DEV_PORT=31421 npm run dev
```

本机网页在 [compounding-life-os](https://github.com/briskkk/compounding-life-os) 另开一个终端执行 `npm run dev`（端口 3000）后，在菜单栏里选择「本机开发 localhost:3000」，或在本仓库启动时指定：

```bash
CLO_BASE_URL=http://localhost:3000 npm run dev
```

明文 HTTP 只接受 `localhost`、`127.0.0.1` 和 `::1`。公网 IP 不能作为服务器地址，请使用域名。

登录态记在系统钥匙串（服务名 `com.huayanpei.family-wealth`）。配置文件只保存服务器地址和上次已经提醒过的待办数量，路径由系统应用支持目录决定，文件名是 `config.json`。

## 打一个本地 .app

在本仓库根目录执行：

```bash
npm run build
```

产物在 `src-tauri/target/release/bundle/macos/家庭财富终端.app`。这个包没有签名，也没有公证，只适合自己的 Mac。不需要登录 App Store。

换一台自建 HTTPS 域名时，把该域名加进 `src-tauri/capabilities/remote-session.json` 的 `remote.urls` 再重新构建。否则网页可以打开，但菜单栏收不到登录令牌。当前允许的地址是 `https://www.huayanpei.com`、`https://huayanpei.com`、`http://localhost:3000` 和 `http://127.0.0.1:3000`。

## 安全

客户端不记录邮箱、密码或令牌。登录钩子只转发登录接口已经返回的 token。远程网页不能调用文件、壳或其它系统接口。
