# xinlan-clip

Linux 桌面剪贴板历史工具：常驻托盘，快捷键唤起无边框面板，点击条目即粘贴；历史保存在本机 SQLite，可搜索、置顶、删除、清空。

**Tauri 2 + React 19 + shadcn/ui + SQLite**

---

## 先决条件

```bash
# Rust 与 Node 工具链（本机装在用户目录，默认不在 PATH 中）
export PATH="$HOME/.cargo/bin:$HOME/.nvm/versions/node/v24.21.0/bin:$HOME/.local/share/pnpm:$PATH"

# Ubuntu 24.04 的 Tauri 系统依赖（本机已全部具备）
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
                 libxdo-dev librsvg2-dev build-essential curl wget file
```

## 开发与构建

```bash
pnpm install
pnpm tauri dev        # 开发模式
pnpm tauri build      # 产出 deb / AppImage
```

---

## 使用方式

| 操作 | 说明 |
| --- | --- |
| `Ctrl+Alt+V` | 唤起面板（需先在设置中登记） |
| 左键单击托盘图标 | 唤起面板 |
| 托盘右键菜单 | 显示 / 设置 / 暂停记录 / 退出 |
| `↑` `↓` | 在列表中移动 |
| `Enter` | 粘贴选中条目 |
| `Ctrl+1` … `Ctrl+9` | 直接跳到第 N 条 |
| `Esc` | 隐藏面板 |
| 输入即搜索 | 在顶部搜索框输入，实时过滤 |

设置中可调整：选择后是否自动粘贴、最多保留多少条、登记/移除全局快捷键。

---

## 重要：两个 GNOME Wayland 平台限制

这两点是**实测结论**，不是实现偷懒。它们决定了本应用的行为，遇到时不必怀疑是 bug。

### 1. 无法后台监听剪贴板 —— 记录是“按需捕获”

Wayland 的安全模型下，后台程序读不到其他应用复制的内容，也拿不到“剪贴板已变化”的通知。本机实测：`org.freedesktop.portal.Clipboard.SelectionOwnerChanged` 这个信号虽然在 D-Bus 接口上存在，但写入剪贴板后监听 20 秒**一个信号都没有发出**（该门户接口在 GNOME 46 上尚未真正生效）。

因此记录时机是固定的三个：

- 面板每次被唤起时；
- 面板重新获得焦点时；
- 面板保持打开期间，每秒轮询一次。

**这意味着：** 面板关闭时复制的内容不会自动进入历史，下次打开面板时会把“当前剪贴板里的那一份”补记进来。连续复制多条时，保持面板打开即可全部记录。

这是 GNOME 46 的限制（GNOME 48 起系统自带剪贴板历史）。KDE / Sway 等实现了 `wlr-data-control` 的合成器上可以做到真正的后台监听，本应用暂未实现该后端。

### 2. 应用无法自行注册全局快捷键

GNOME 46 未实现 `org.freedesktop.portal.GlobalShortcuts`（本机 `xdg-desktop-portal` 1.18.4 上该接口**不存在**），而 Tauri 的 global-shortcut 插件在 Linux 上走的是 X11 `XGrabKey`，对 Wayland 客户端无效。所以本应用**不会**自己抢占按键。

取而代之的是：设置页里的“登记”按钮会通过 `gsettings` 写入一条 GNOME 自定义快捷键，命令为

```
<可执行文件路径> --toggle
```

再次启动时由单实例插件转交给正在运行的进程并弹出面板。这条快捷键会出现在 GNOME「设置 → 键盘 → 自定义快捷键」中，可随时改键或删除。

> 登记后若按键无反应，注销并重新登录一次即可生效（GNOME 需要刷新快捷键注册）。

### 3. 自动粘贴走 RemoteDesktop 门户（本机实测不能真正送达）

点击条目时，应用会先把内容写回剪贴板，**隐藏面板**（把焦点交还给原来的窗口），再通过 `org.freedesktop.portal.RemoteDesktop` 的 `NotifyKeyboardKeysym` 合成一次 `Ctrl+V`。

**本机实测结论（重要）：** GNOME 46 的该门户**接受**这些调用但**不会真正投递按键**：

- `CreateSession` / `SelectDevices` / `Start` 均返回响应码 0，并报告 `devices = 3`；
- `NotifyKeyboardKeysym` 按下/抬起都返回成功，没有任何报错；
- 但一个 Wayland 原生窗口在注入时刻 `is_active() == true`，却收不到任何 `key-press-event`；XWayland 窗口（持有 X11 焦点）同样收不到。

因此**自动粘贴在本机大概率不生效**。这不是本应用的实现错误（调用路径完全正确，也是官方推荐机制），而是 GNOME 46 该门户尚未真正接通。行为设计为：

- 无论自动粘贴是否成功，内容**一定**已经写入系统剪贴板；
- 面板再次打开时会提示“已复制到剪贴板”，此时直接按 `Ctrl+V` 即可；
- 若不需要自动粘贴，可在设置中关闭该开关；
- 在其他正确实现了该门户的桌面环境上，同一套代码无需改动即可自动粘贴。

终端类程序里 `Ctrl+V` 语义不同，可改用 `Ctrl+Shift+V`。

---

## 数据

历史存放在：

```
~/.local/share/com.xinlan.clip/clipboard.db
```

- 单表 `entries`，只存纯文本（v1 范围）。
- 重复内容按内容哈希去重，只累加使用次数、刷新时间，不产生新行。
- 超过“最多保留”条数时，自动清理**最久未使用**的非置顶记录；置顶条目不计入上限。
- 全部内容留在本机，不会上传。

**注意：** v1 没有敏感内容过滤，密码管理器等写入剪贴板的机密内容也会被记录。如不需要，可用托盘菜单「暂停记录」临时关闭。

清除数据：

```bash
rm -f ~/.local/share/com.xinlan.clip/clipboard.db*
```

---

## 项目结构

```
src/                      前端（React + shadcn/ui）
  App.tsx                 面板外壳：列表 / 设置切换、事件、键盘导航
  components/             EntryList、EntryItem、PanelHeader/Footer、SettingsView、HintBanner
  components/ui/          shadcn 生成的基础组件
  hooks/useEntries.ts     历史列表状态；按需捕获与轮询
  hooks/useKeyboardNav.ts 全局方向键 / Enter / Esc / Ctrl+数字
  lib/ipc.ts              Tauri 命令的类型化封装
  styles/globals.css      Tailwind v4 主题、窗口外壳、视图过渡与 reduced-motion

src-tauri/src/
  lib.rs                  入口：迁移、托盘、单实例、焦点处理、门户引导
  commands.rs             全部 #[tauri::command]
  db.rs                   SQLite 读写、去重、清理（含单元测试）
  clipboard.rs            arboard 读写 + 自身写入抑制
  portal.rs               RemoteDesktop 门户：输入会话与按键合成
  gnome_hotkey.rs         gsettings 快捷键登记（含单元测试）
  settings.rs             设置读写
  app-icon.png            图标源文件（1024×1024）
  icons/                  由图标源生成的各尺寸产物
```

## 图标

图标源文件是 `src-tauri/app-icon.png`（1024×1024，深靛蓝渐变圆角底 + 白色剪贴板）。
修改图标后重新生成各尺寸：

```bash
pnpm tauri icon src-tauri/app-icon.png
```

该命令会同时生成 Linux 所需的 `32x32`/`128x128`/`128x128@2x` 与 `icon.png`。
托盘图标复用的是窗口图标（`app.default_window_icon()`），因此会一并更新。

## 测试

```bash
cd src-tauri && cargo test        # 数据层与快捷键解析的单元测试
pnpm exec tsc --noEmit            # 前端类型检查
```

## 已知限制

- 仅支持纯文本；图片、富文本不记录。
- 面板窗口固定居中：Wayland 不允许应用自行移动窗口，因此没有做“跟随鼠标弹出”。
- 面板打开时会每秒读一次剪贴板，用于发现新的复制内容。
- 未做密码等敏感内容过滤（见上）。
