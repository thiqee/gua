# KeyHop

快捷键启动器。按下热键弹出搜索框，输入识别码，快速打开网址、文件、文件夹或启动程序。支持自定义热键、搜索引擎、黑名单、IME 中文输入、配置热重载。

---

## 当前状态

**平台：** Windows

**全部功能已完成，持续迭代中。**

---

## 技术选型

| 层 | 选型 | 说明 |
|---|---|---|
| 语言 | Rust | edition 2021 |
| UI | 纯 Win32 API（自绘窗口 + 自绘输入框 + 自绘列表） | 零 UI 框架依赖，内存 DC 双缓冲消除闪烁 |
| 全局热键 | Win32 `RegisterHotKey`（FFI）+ 外部函数声明 | 支持自定义组合键 Alt/Ctrl/Shift/Win + 任意键 |
| 系统托盘 | Win32 `Shell_NotifyIcon`（FFI） | 右键菜单 |
| 配置解析 | 手写 TOML 子集 | 零依赖，极简 |
| 字体渲染 | GDI（`CreateFontW` + `DrawTextW`） | 支持自定义字体和字号 |
| 窗口圆角 | `SetWindowRgn`（`CreateRoundRectRgn`） | 兼容所有 Windows 版本 |
| 列表圆角 | GDI+（`GdipFillPath`） | 选中项高亮圆角 |
| 前台进程检测 | `GetForegroundWindow` + `QueryFullProcessImageNameW`（FFI） | 黑名单功能 |
| 托盘图标 | `LoadImageW` 加载外置 `KeyHop.ico`，无文件则用系统默认 | 支持自定义替换 |

### 依赖

仅 `windows` crate（v0.62），无其他 Rust 依赖。

---

## 架构

```
src/
├── main.rs      — 入口：窗口注册、消息循环、GDI+ 初始化、热键注册
├── config.rs    — 配置加载（解析 config.toml → Vec<Entry>）
├── state.rs     — 数据结构、常量、工具函数、热键解析、黑名单检测
├── window.rs    — 窗口管理：显示/隐藏、列表过滤、执行、光标、黑名单检查
├── draw.rs      — 自绘函数：GDI+ 圆角矩形、列表项绘制、状态栏绘制
├── wndproc.rs   — 窗口过程：WM_PAINT、WM_CHAR、WM_KEYDOWN、IME 消息、热键响应
├── executor.rs  — 执行器（识别码 → 打开 URL / 启动程序 / 打开文件或文件夹）
└── tray.rs      — 系统托盘图标与右键菜单
```

---

## 功能

### 搜索启动

- 按下热键（默认 `Alt+Space`）弹出搜索框，自动聚焦输入框
- 输入识别码，下方实时过滤显示匹配条目（前缀匹配）
- 方向键 `↑` `↓` 选择，`Enter` 执行
- 执行后自动隐藏窗口
- `Esc` 或窗口失去焦点 → 隐藏

### 自定义热键

支持任意组合键，配置 `_hotkey` 即可：

```toml
_hotkey = Ctrl+J
_hotkey = Ctrl+Shift+F
_hotkey = Alt+F1
```

- 至少两个键，第一个必须是修饰键（Alt / Ctrl / Shift / Win）
- 不区分大小写
- 热重载时自动重新注册，无需重启

### 黑名单

指定某些程序在前台时热键不响应，防止游戏等场景误触：

```toml
_blacklist = notepad.exe,calc.exe
```

- 逗号分隔多个 exe 文件名
- 不区分大小写
- 热重载时同步更新

### 搜索引擎

配置文件中添加带 `?q=` 或 `?wd=` 参数的 URL 条目，输入识别码加空格加关键词即可搜索：

```toml
[搜索引擎]
gg = https://www.google.com/search?q=
bd = https://www.baidu.com/s?wd=
```

用法：`gg 今天天气` → 打开 `https://www.google.com/search?q=今天天气`

### 输入框光标操作

| 按键 | 效果 |
|---|---|
| `←` `→` | 在输入文本中移动光标 |
| `Backspace` | 删除光标前一个字符 |
| `Delete` | 删除光标后一个字符 |
| 输入字符 | 在光标位置插入 |

支持中文等 IME 输入法，拼写过程中预编辑文字在输入框内实时显示。

### 值类型自动判断

| 特征 | 行为 |
|---|---|
| 以 `http://` 或 `https://` 开头 | 默认浏览器打开 |
| 以 `.exe` 结尾 | 启动该程序 |
| 已存在的文件夹路径 | 打开文件夹 |
| 已存在的文件路径 | 默认程序打开 |
| 不存在 | 报错，不执行 |

### 系统托盘

- 启动时隐藏到系统托盘
- 右键菜单：打开 KeyHop / 打开配置文件 / 退出
- 左键点击或按热键弹出窗口
- 点击窗口关闭按钮（X）→ 隐藏而非退出

### 配置文件热重载

- 每次按热键弹出时自动检查 `config.toml` 修改时间
- 有变更则立即重新加载，无需重启程序
- 热键、黑名单等所有配置项均支持热重载

---

## 经验教训：`DrawTextW` 传入空 `&mut [u16]` 导致崩溃（0xC000041D）

### 现象

- 程序启动后按 `Alt+Space` 呼出，立即闪退
- 退出码 `0xC000041D`（`STATUS_FATAL_USER_CALLBACK_EXCEPTION`）
- 崩溃地址在系统 DLL 内部（`0xC0000005` 访问违规），不在程序代码中

### 排查过程

1. `panic_hook` 没触发 → 不是 Rust panic，是 SEH 异常
2. `SetUnhandledExceptionFilter` 捕获到 `0xC0000005` 访问违规，地址固定在系统 DLL 内
3. 日志改为**追加模式**，看到完整执行流程：`toggle_win` 执行完毕 → `WM_PAINT` 消息中崩溃
4. 在 `WM_PAINT` 每个 GDI 调用前后加日志 → 定位到 `DrawTextW` 崩溃
5. 参数检查：HDC 有效（前面 `FillRect` 成功了）、矩形有效、但字符串是**空**的

### 根因

```rust
// 错误写法
let mut ws: Vec<u16> = s.input_text.encode_utf16().collect();
DrawTextW(hdc, &mut ws, &mut rect, flags);
```

- `s.input_text` 初始为空字符串 → `encode_utf16()` 产生空迭代器 → `ws` 是空 `Vec<u16>`（`[]`）
- `windows` crate 的 `DrawTextW` 包装接收 `&mut [u16]`，底层传给 Win32 API 时 `cchText=0`
- `DrawTextW` 在某些系统版本上对 `cchText=0` 且无 null 终止符的空切片处理异常，导致系统 DLL 内访问违规

### 修复

```rust
// 正确写法
let mut ws: Vec<u16> = s.input_text.encode_utf16().collect();
ws.push(0);  // 确保始终有 null 终止符
if !ws.is_empty() && rect.right > rect.left && rect.bottom > rect.top {
    DrawTextW(hdc, &mut ws, &mut rect, flags);
}
```

### 教训

**传给 Win32 API 的切片/指针必须考虑空值情况。** 尤其是字符串参数：
1. 始终确保 null 终止（`.push(0)`），即使字符串为空
2. 空字符串时主动跳过调用，而不是靠系统 API 内部容错
3. 所有 GDI 绘制函数调用前，检查矩形有效性（`right > left && bottom > top`）
4. 调试崩溃时，日志要用**追加模式**（`OpenOptions::append(true)`），覆盖写会丢失关键信息
5. 回调中的崩溃不一定是 Rust panic → 需要 SEH 异常捕获才能定位

---

## 经验教训：中文输入导致 `STATUS_STACK_BUFFER_OVERRUN`（0xC0000409）

### 现象

- 输入中文时程序闪退，退出码 `0xC0000409`（`STATUS_STACK_BUFFER_OVERRUN`）
- 没有 panic.log（不是 Rust panic），没有 crash.log（不是 SEH 异常）

### 排查过程

1. 在 `WM_CHAR` 每步操作前后加日志 → 定位到 `update_caret` 内部崩溃
2. 崩溃点：`s.input_text[..s.cursor_pos]` 字符串切片

### 根因

```rust
// 错误写法
let ch = '阿';  // UTF-8 编码占 3 字节
s.input_text.insert(s.cursor_pos, ch);  // input_text = "阿"（3 字节）
s.cursor_pos += 1;  // 只加了 1，但实际占了 3 字节
// ...
let prefix = &s.input_text[..s.cursor_pos];  // ← 取第 1 个字节，不是字符边界
// PANIC: byte index 1 is not a char boundary
```

- 中文在 Rust 的 `String` 中按 UTF-8 存储，一个字占 2~3 字节
- `cursor_pos` 被当作字符位置（每次 +1），但 `String` 索引是按字节的
- 取 `input_text[..1]` 切入多字节字符内部 → Rust 报 panic
- panic 发生在 `extern "system" fn wndproc` 内，无法跨 FFI 边界展开 → `STATUS_STACK_BUFFER_OVERRUN`

### 修复

所有按 `cursor_pos` 跳转的地方改为按 UTF-8 字节边界跳：

```rust
// 插入时按实际字节数进位
s.cursor_pos += ch.len_utf8();

// 左移：跳到前一个字符的字节起始
s.cursor_pos = s.input_text.floor_char_boundary(s.cursor_pos - 1);

// 右移：跳到后一个字符的字节起始
s.cursor_pos = s.input_text.ceil_char_boundary(s.cursor_pos + 1);

// 退格：找到上一个字符边界再删除
let prev = s.input_text.floor_char_boundary(s.cursor_pos - 1);
s.input_text.replace_range(prev..s.cursor_pos, "");
s.cursor_pos = prev;

// 删除：找到下一个字符边界
let next = s.input_text.ceil_char_boundary(s.cursor_pos + 1);
s.input_text.replace_range(s.cursor_pos..next, "");
```

### 教训

1. Rust 的 `String` 索引按**字节**不是按**字符**，处理中文时牢记这点
2. `extern "system" fn` 内的 panic 无法正常展开 → 导致进程终止，不易调试
3. 跨 FFI 边界的 panic 表现为 `STATUS_STACK_BUFFER_OVERRUN`，不是常规的 panic 日志
4. 必须用 `floor_char_boundary` / `ceil_char_boundary` 处理 UTF-8 边界

---

## 配置文件格式

`config.toml`，支持分类分组和内部配置键。

### 识别码条目

```toml
[网址]
b23  = https://www.bilibili.com
gh   = https://github.com

[程序]
calc    = C:\Windows\System32\calc.exe
notepad = C:\Windows\System32\notepad.exe

[搜索引擎]
gg = https://www.google.com/search?q=
bd = https://www.baidu.com/s?wd=
```

规则：
- `#` 开头的行是注释（必须独占一行）
- 空行被忽略
- `=` 左右允许空格
- `[分类名]` 仅用于分组，不影响搜索逻辑
- 以 `_` 开头的 key 为内部配置键

---

## 内部配置键一览

| 配置键 | 说明 | 默认值 |
|---|---|---|
| `_hotkey` | 热键组合 | `Alt+Space` |
| `_blacklist` | 黑名单程序（逗号分隔 exe 名） | 空（不过滤） |
| `_font` | 字体名称 | `Segoe UI` |
| `_font_size` | 字号 | `18` |
| `_status_font_size` | 状态栏字号 | `12` |
| `_width` | 窗口宽度（像素） | `500` |
| `_round_corner` | 窗口圆角大小 | `12` |
| `_max_results` | 列表最多显示条数 | `8` |
| `_theme_color` | 窗口背景色（RGB 十六进制） | `1E1E1E` |
| `_input_bg_color` | 输入框背景色 | `2A2A2A` |
| `_accent_color` | 选中项高亮色 | `4A6FA5` |
| `_text_color` | 文字颜色 | `CCCCCC` |
| `_opacity` | 窗口透明度（0~255） | `255` |
| `_always_on_top` | 窗口置顶 | `true` |
| `_case_sensitive` | 匹配区分大小写 | `true` |
| `_hide_on_focus_loss` | 失去焦点自动隐藏 | `true` |
| `_panel_position_x` | 面板水平位置（0~100） | `50` |
| `_panel_position_y` | 面板垂直位置（0~100） | `50` |

颜色值用 6 位十六进制（RGB），如 `FF0000` 红色、`00FF00` 绿色。

---

## 构建 & 运行

```bash
# 构建（仅 Windows）
cargo build --release

# 运行
cargo run --release
```

编译产物为单一 exe（release 配置已启用 LTO、strip、尺寸优化）。


---

## 经验教训：GDI 自绘列表选中高亮闪烁

### 现象

上下键切换列表选中项时，高亮偶尔闪回到上一个选中项。WM_SETREDRAW + 全刷 ListBox 时整个面板闪烁，包括状态栏。

### 排查过程

1. 初始：`SendMessageW(list_hwnd, WM_KEYDOWN)` → ListBox 发送 2 个 `WM_DRAWITEM`（旧项+新项），分两次绘制到屏幕，中间状态可见
2. 尝试内存 DC 双缓冲（每项单独 `CreateCompatibleDC` + `BitBlt`）→ 每项单独 BitBlt 反而放大了中间状态
3. 尝试 `WM_SETREDRAW(0)` → `WM_KEYDOWN` → `WM_SETREDRAW(1)` + 全刷 ListBox → 高亮不闪了，但整个面板闪烁
4. 最终：**放弃 ListBox，完全自绘列表**

### 根因

GDI 没有图层概念。每次 `FillRect`、`FillRgn`、`DrawTextW` 直接写像素到屏幕，后写的覆盖先写的。`WM_DRAWITEM` 逐个发送，中间状态暴露在屏幕上。

### 最终方案

**不再使用 ListBox 控件，直接在 WM_PAINT 中自绘列表。**

- 所有条目在 `WM_PAINT` 中一次画完，不依赖 `WM_DRAWITEM`
- 上下键切换选中时，用 `GetDC` 直接拿窗口画布，只重绘旧选中项和新选中项以及状态栏，不经过 `InvalidateRect` 和 `WM_PAINT`
- 不重绘文字内容不变的条目（切换选中时文字内容不变，只有高亮变）
- 先 `FillRgn` 擦/画高亮圆角，再 `DrawTextW` 重写文字（因为 FillRgn 覆盖了圆角区域内的像素，包括文字）

```rust
// VK_UP/VK_DOWN 核心逻辑
let dc = GetDC(Some(h));
// 旧选中项（擦高亮 + 重写文字）
draw_item_hl_text(dc, s, old_sel, &item_rect, false);
// 新选中项（画高亮 + 重写文字）
draw_item_hl_text(dc, s, new_sel, &item_rect, true);
// 状态栏（清背景 + 重写文字）
FillRect(dc, &status_rect, bg_brush);
DrawTextW(dc, &mut status_text, &mut status_rect, DT_RIGHT);
let _ = ReleaseDC(Some(h), dc);
```

### 教训

1. **不要依赖 ListBox 的 `WM_DRAWITEM` 做选中切换** — 一定会有闪烁，因为两次绘制之间屏幕显示中间状态
2. **`WM_SETREDRAW` + 全刷** 虽然解决高亮闪烁，但会导致面板整体闪烁
3. **小范围更新用 `GetDC` 直接画**，不走 `InvalidateRect` 和 `WM_PAINT`，避免消息队列延迟和不必要的全量重绘
4. **GDI 的 `FillRgn` / `FillRect` 会覆盖区域内的所有像素**（包括文字），改背景后必须重写文字
5. **状态栏和列表文字**如果内容没变（如上下键只变选中位置，共y条不变），不应重绘，减少绘制量

---

## 经验教训：`InvalidateRect` 异步重绘导致输入/退格时面板闪烁

### 现象

输入字符、按退格或删除键时面板闪烁。

### 排查

涉及 `WM_CHAR`、`VK_BACK`、`VK_DELETE` 的处理流程：

```
fill_list(s, h)  ← SetWindowPos 改变窗口高度
InvalidateRect(Some(h), None, true)
```

`InvalidateRect` 的问题：
- **异步**：标记无效后要等消息循环下次才处理 `WM_PAINT`，而 `SetWindowPos` 已改变窗口大小并更新了屏幕显示，新区域在 `WM_PAINT` 前是未绘制的
- **`bErase=true`**：`BeginPaint` 先擦背景再画前景，`WM_PAINT` 虽有全窗口覆盖绘制，但擦画之间产生中间空白帧

### 修复

将 5 处 `InvalidateRect(Some(h), None, true)` 替换为：
```rust
RedrawWindow(Some(h), None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
```

- `RDW_UPDATENOW` — 同步立即触发 `WM_PAINT`，在函数返回前完成绘制
- `RDW_NOERASE` — 不擦除背景（`WM_PAINT` 已全窗口覆盖）

### 第一次修复（半程）

上述改动消除了异步延迟 + 预擦背景导致的闪烁，但输入/退格时面板仍有闪烁，因为问题还有第二层。

### 第二次修复：`WM_PAINT` 内存 DC 双缓冲

#### 剩余闪烁的原因

`WM_PAINT` 内所有 GDI 绘制都是逐个直接写屏的：

```
FillRect(窗口背景)       → 屏幕显示纯背景
fill_round_rect(输入框)  → 屏幕显示背景+输入框框体
DrawTextW(输入框文字)    → 屏幕显示背景+完整输入框
draw_filtered_item(列表) → 逐条出现
redraw_status_bar(状态栏)→ 最终完整窗口
```

每个函数调用之间的中间状态都暴露在屏幕上，产生闪烁。

#### 修复

在 `WM_PAINT` 中使用内存 DC 双缓冲，所有绘制在内存中完成，最后一次性复制到屏幕：

```rust
let mem_dc = CreateCompatibleDC(Some(hdc));
let bmp = CreateCompatibleBitmap(hdc, width, height);
SelectObject(mem_dc, HGDIOBJ(bmp.0));
// 以下所有绘制函数的 hdc 参数改为 mem_dc
// ...
// 一次性复制到屏幕
BitBlt(hdc, 0, 0, width, height, Some(mem_dc), 0, 0, SRCCOPY);
// 清理
SelectObject(mem_dc, old_bmp);
DeleteObject(HGDIOBJ(bmp.0));
DeleteDC(mem_dc);
```

#### 效果

输入、退格、删除时面板无任何闪烁，彻底解决。

### 教训

1. **GDI 直接写屏是闪烁的根本原因**，每个绘制调用都直接暴露中间状态
2. **内存 DC 双缓冲**是 GDI 下消除闪烁的标准方案：在内存中画完完整一帧，一次 `BitBlt` 到屏幕，用户只看到最终结果
3. 双缓冲需要提前知道窗口完整尺寸（`CreateCompatibleBitmap` 需要宽高），`WM_PAINT` 中可通过 `win_h()` 计算
4. `SelectObject` 进出要配对，注意 `HBITMAP` 到 `HGDIOBJ` 的类型转换
5. 两层问题要分层排查：第一层是异步延迟 + 预擦背景，第二层是逐个写屏的中间状态，不能混为一谈

---

## 代码审查须知

以下问题已知且决定不处理，审查代码时无需提出：

- **`SetProcessDPIAware` 过时** — windows crate 0.62 中没有导出新 API，旧 API 在 Win10/11 上正常工作。
- **`#![allow(unused_must_use)]`** — Win32 API 调用失败时程序无法恢复，加 `let _ =` 只会增加噪声，不会提升安全性。
- **`RegisterHotKey` / `SetFocus` 重复 extern 声明** — windows crate 中对应符号名不确定，无法直接替换。

---

## 构建配置

`Cargo.toml` release 优化：

```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = "symbols"
opt-level = "z"
```
