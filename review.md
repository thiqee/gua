# Gua 项目全面审查报告

**项目**: E:/projects/gua — Rust + Win32 API Windows 桌面搜索启动器\
**版本**: 0.1.3\
**审查日期**: 2026-06-21\
**审查范围**: `src/main.rs`, `src/config.rs`, `src/state.rs`, `src/window.rs`, `src/wndproc.rs`, `src/draw.rs`, `src/executor.rs`, `src/tray.rs`\
**审查流程**: `scout`（代码扫描）→ `oracle`（挑战验证）→ `reviewer`（最终审查）

---

## 整体评价

Gua 是一个用 Rust + 纯 Win32 API 构建的 Windows 桌面快速启动器，代码量约 960 行（不含注释/空行），8 个源文件，结构清晰，职责划分合理。

**做得好的地方**：

- 消息循环正确，WM_PAINT 内存 DC 双缓冲消除闪烁
- IME 中文输入处理到位（`WM_IME_STARTCOMPOSITION` / `WM_IME_COMPOSITION` / `WM_IME_ENDCOMPOSITION` 完整处理）
- 配置热重载逻辑完备（mtime 检查 + 全量热更新）
- 拼音匹配引擎实现 6 级匹配策略，含多音字覆写
- 托盘点击防抖（`last_hide_time` 机制）
- UTF-8 字符边界处理正确（`floor_char_boundary` / `ceil_char_boundary`）
- 开发过程文档化充分（DEVELOPER.md 含多次调试记录和经验教训）

**主要问题集中在**：

1. 资源清理不完整（GDI 对象泄漏、堆内存泄漏）
2. 若干 Win32 API 参数缺少边界容错
3. 列表项 `&` 字符显示错误

---

## P0 — 必须修复

### 1. `#![allow(unused_must_use)]` 全局忽略返回值

**位置**: `src/main.rs:3`、`src/tray.rs:4`

**说明**: 两个文件均使用 `#![allow(unused_must_use)]`，导致编译器不警告被忽略的 `Result` 返回值。关键静默失败点：

| 位置 | 调用 | 失败影响 |
| --- | --- | --- |
| `executor.rs:34` | `CreateProcessW` | 命令执行失败无反馈 |
| `executor.rs:46-47` | `CloseHandle` | 句柄泄漏 |
| `executor.rs:60` | `ShellExecuteW` | 打开 URL/文件失败无反馈 |
| `main.rs:68` | `SetProcessDPIAware` | DPI 感知可能未生效 |
| `main.rs:72` | `ReleaseDC` | DC 泄漏 |
| `main.rs:224` | `CloseHandle(mutex)` | 互斥体句柄泄漏 |
| `tray.rs:89` | `GetCursorPos` | 菜单位置可能错误 |
| `tray.rs:102` | `DestroyMenu` | 菜单资源泄漏 |

**修复建议**: 移除全局 allow，关键调用点逐处加 `let _ =` 或检查返回值。至少对 `CreateProcessW` 和 `ShellExecuteW` 做错误处理。

---

### 2. `Box::into_raw` 分配的 AppState 在 WM_DESTROY 中从未释放

**位置**: `src/main.rs:182`（`Box::into_raw` 分配）→ `src/wndproc.rs:75`（`WM_DESTROY` 未恢复）

**说明**: `main.rs` 中 `Box::into_raw(Box::new(state))` 将 AppState 转为裸指针存入 `GWLP_USERDATA`。`WM_DESTROY` 只调用了 `tray::destroy()` 和 `PostQuitMessage(0)`，从未执行 `Box::from_raw` 恢复为 Box 以执行 Drop。导致：

- AppState 中所有 `String`/`Vec`/`HashMap` 堆内存泄漏
- `hfont: Option<HFONT>` 和 `status_hfont: Option<HFONT>` 中的 GDI 字体对象未被 `DeleteObject` 释放

**修复建议**: 在 `WM_DESTROY` 末尾添加：

```rust
let ptr = GetWindowLongPtrW(h, GWLP_USERDATA);
if ptr != 0 {
    drop(Box::from_raw(ptr as *mut AppState));
    SetWindowLongPtrW(h, GWLP_USERDATA, 0);
}
```

---

### 3. 托盘图标 HICON 从未调用 `DestroyIcon`

**位置**: `src/tray.rs:30-39`（`load_ico_from_bytes` → `CreateIconFromResourceEx`），第 56 行赋值给 `nid.hIcon`

**说明**: `load_ico_from_bytes` 通过 `CreateIconFromResourceEx` 创建 HICON。该图标被设置到 `NOTIFYICONDATAW.hIcon` 后，`Shell_NotifyIconW` 仅从系统托盘删除图标条目，不负责释放 HICON 资源。代码中从未调用 `DestroyIcon`。

**修复建议**: 将 HICON 存为静态变量或存入 AppState，在 `destroy()` 中调用 `DestroyIcon(hicon)`。

---

### 4. 列表项 `DrawTextW` 缺少 `DT_NOPREFIX`

**位置**:

- `src/draw.rs:90` — `draw_item_hl_text` 中的 `DrawTextW`
- `src/draw.rs:121` — `draw_filtered_item` 中的 `DrawTextW`

两个调用的 flags: `DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS`（均缺少 `DT_NOPREFIX`）

**说明**: `DrawTextW` 默认将 `&` 解释为加速键前缀（为后续字符加下划线）。当条目值或描述中包含 `&` 时（例如 URL `https://example.com?a=1&b=2`），显示异常。输入框的 `WM_PAINT`（wndproc.rs:59-60）已通过 `replace("&", "&&")` 正确转义，但列表两处自绘函数未做同样处理，**两处不一致**。

**修复建议**: 在两个 `DrawTextW` 调用中添加 `DT_NOPREFIX` flag，或在调用前对 `&` 做 `replace("&", "&&")` 转义。推荐加 `DT_NOPREFIX`。

---

## P1 — 建议修复

### 5. UTF-8 BOM 导致 config.toml 第一个区段头被吞

**位置**: `src/config.rs:30`（`content.lines()`），第 37 行（`line.starts_with('[')`）

**说明**: Rust 的 `lines()` 不会自动剥离 UTF-8 BOM（字节序列 `EF BB BF`）。如果 config.toml 以 BOM 开头（记事本默认行为），第一个 `[section]` 行变成 `\u{FEFF}[section]`，`line.starts_with('[')` 返回 `false`，导致第一个分类被静默吞掉。

当前项目 config.toml 无 BOM，但用户编辑配置时可能引入。

**修复建议**: 在 `load()` 函数开头添加：

```rust
let content = content.trim_start_matches('\u{FEFF}');
```

---

### 6. `SetForegroundWindow` 可能失败（UIPI / 全屏程序）

**位置**: `src/window.rs:237`（`toggle_win` 中的 `SetForegroundWindow(h)`）

**说明**: Windows Vista+ UIPI 限制低特权进程不能 `SetForegroundWindow` 高特权进程的窗口。当全屏游戏（通常以管理员权限运行）处于前台时，`SetForegroundWindow` 可能失败，导致面板弹出但无法自动获得输入焦点。

**修复建议**: 调用 `SetForegroundWindow` 后检查 `GetForegroundWindow()` 是否为本窗口，若不成功可用 `AttachThreadInput` 辅助。

---

### 7. `read_font_family` 字体文件偏移量边界检查不完整

**位置**: `src/state.rs:240-320`

**说明**: `read_font_family` 在解析字体文件时使用 `data.get()` 读数据，但在计算字符串偏移时：

```rust
let start = string_off + c.offset;    // 可能溢出（release 下 wrap）
if start + c.length > nt.len() { ... }  // 溢出后通过检查
let raw = &nt[start..start + c.length];
```

虽然当前通过 `get()` 边界检查安全，但加法溢出在 release 模式下会 wrap 而非 panic，可能绕过后续检查。

**修复建议**: 使用 `checked_add`/`checked_sub`：

```rust
let start = string_off.checked_add(c.offset)?;
let end = start.checked_add(c.length)?;
if end > nt.len() { continue; }
let raw = &nt[start..end];
```

---

### 8. `static mut TRAY_HWND` — Rust unsound

**位置**: `src/tray.rs:21` — `static mut TRAY_HWND: HWND = HWND(ptr::null_mut());`

**说明**: Rust 的 `static mut` 是语言中已知 unsound 的构造。在 LLVM 严格别名模型下，通过 `addr_of_mut!` 之外的写操作可能被优化器误判。当前单线程消息循环下实际安全，但不符合 Rust 安全代码规范，且将来引入多线程就是数据竞争 UB。

**修复建议**: 使用 `OnceLock<HWND>`（stable since Rust 1.70）：

```rust
use std::sync::OnceLock;
fn tray_hwnd() -> &'static OnceLock<HWND> {
    static HWND: OnceLock<HWND> = OnceLock::new();
    &HWND
}
```

或将 HWND 存入 AppState 而非全局变量。

---

## P2 — 代码质量

### 9. `draw_item_hl_text` 与 `draw_filtered_item` 高度重复

**位置**: `src/draw.rs:68-97`（`draw_item_hl_text`）和 `src/draw.rs:99-127`（`draw_filtered_item`）

**说明**: 两个函数 90% 逻辑相同：获取条目 → 格式化文本 → 选字体 → 设置颜色 → DrawTextW → 恢复字体。区别仅在于背景绘制策略（`draw_item_hl_text` 用 `fill_round_rect`，`draw_filtered_item` 先 `FillRect` 再条件画高亮）。

**修复建议**: 提取公共部分为辅助函数，接受 `selected: bool` 和 `full_redraw: bool` 参数。

---

### 10. `cfg_bool` 只认小写 `"true"` / `"1"`

**位置**: `src/state.rs:52`

```rust
e.value == "true" || e.value == "1"
```

**说明**: 不识别 `"True"`、`"TRUE"`、`"Yes"`、`"yes"`、`"on"` 等常见布尔值表示。

**修复建议**: 使用 `e.value.eq_ignore_ascii_case("true")`。

---

### 11. `cfg_color` 不支持 `#` 前缀

**位置**: `src/state.rs:55-58`

**说明**: `u32::from_str_radix(&e.value, 16)` 直接解析，用户写 `#FF0000` 时 `#` 导致解析失败静默回退。

**修复建议**:

```rust
let val = e.value.trim_start_matches('#');
u32::from_str_radix(val, 16).ok()
```

---

### 12. 输入框 `&` 转义与光标测量不一致

**位置**:

- `src/wndproc.rs:59` — WM_PAINT 中 `replace("&", "&&")` 后再 DrawTextW
- `src/window.rs:262-273` — `update_caret` 中用原始未转义文本测量宽度

**说明**: `update_caret` 使用 `&s.input_text[..s.cursor_pos]` 测量文本宽度。如果输入文本含 `&`，WM_PAINT 渲染的是转义后的 `&&`（更宽），而光标测量用原文（更窄），导致光标位置与实际字符位置错位。

**修复建议**: `update_caret` 中也对前缀文本做 `replace("&", "&&")` 后再测量。

---

### 13. `SetWindowRgn` 失败时 HRGN 泄漏

**位置**: `src/state.rs:134-137`

```rust
let rgn = CreateRoundRectRgn(0, 0, w, hh, corner, corner);
if !rgn.is_invalid() {
    SetWindowRgn(h, Some(rgn), true);
}
```

**说明**: MSDN 规定只有 `SetWindowRgn` **成功**后系统才接管 region 句柄所有权。失败时 region 不会被自动释放，导致 GDI 对象泄漏。

**修复建议**:

```rust
if !rgn.is_invalid() {
    if SetWindowRgn(h, Some(rgn), true).0 == 0 {
        DeleteObject(HGDIOBJ(rgn.0));
    }
}
```

---

### 14. 缺少 Home/End/PageUp/PageDown 导航键

**位置**: `src/wndproc.rs:158-220` — `WM_KEYDOWN` 处理

**说明**: 当前仅支持 `↑`/`↓`。缺少：

- `Home` (VK_HOME, 0x24) — 跳转到列表第一项
- `End` (VK_END, 0x23) — 跳转到列表最后一项
- `PageUp` (VK_PRIOR, 0x21) — 向上翻一页
- `PageDown` (VK_NEXT, 0x22) — 向下翻一页

**修复建议**: 在 `WM_KEYDOWN` 中添加对应按键的处理。

---

### 15. `RegisterClassW` 返回值未检查

**位置**: `src/main.rs:108`

**说明**: 失败时返回 0，后续 `CreateWindowExW` 也会失败，但错误原因难以追溯。

**修复建议**: 检查返回值并及早报错。

---

### 16. 配置解析失败静默返回空列表

**位置**: `src/config.rs:23-25`

**说明**: `match fs::read_to_string(path)` 的 `Err(_)` 分支返回 `Vec::new()`，用户在 config.toml 损坏或路径错误时得不到任何提示。

**修复建议**: 在 `Err` 分支写入 `panic.log`，或改用 `Result<Vec<Entry>, String>`。

---

### 17. `fill_list` 和 `toggle_win` 重复 `SetWindowPos`

**位置**: `src/window.rs`

**说明**: `toggle_win` 中先调 `fill_list`（内部可能调 `SetWindowPos` 改高度），后可能调 `center_win`（再次 `SetWindowPos`），第一次调用被第二次覆盖。虽用户不可见闪烁（窗口在 `ShowWindow` 前已隐藏），但代码气味明显。

**修复建议**: 先将最终尺寸确定后再做一次 `SetWindowPos`。

---

### 18. `unsafe fn` 缺少 `// Safety:` 文档

**位置**: 所有 `pub unsafe fn`（`state.rs` 中 `round_win`, `center_win`, `make_font_with`, `get_foreground_exe`, `load_private_fonts`；`window.rs` 中多个；`wndproc.rs` 中 `wndproc`；`draw.rs` 中所有函数）

**说明**: Rust 中 `pub unsafe fn` 要求在文档中说明调用者必须维护哪些不变量。当前无一函数包含 `// Safety:` 注释。

**修复建议**: 为每个 `pub unsafe fn` 补充 Safety 说明。

---

## P3 — 边缘/次要

| \# | 问题 | 位置 | 说明 |
| --- | --- | --- | --- |
| 19 | 反引号命令/ShellExecuteW 无校验 | `executor.rs` | config 由用户控制，风险低 |
| 20 | 项目根目录有调试残留文件 | 根目录 | `nul`、`hook.log`(55KB)、`keydown.log`、`wm_char.log` |
| 21 | `eprintln!` 在 `windows_subsystem` 下不可见 | 全局多处 | 已知（DEVELOPER.md 已记录），严重错误可改用 `MessageBoxW` |

---

## Oracle 纠正记录

审查过程中 `oracle` 子代理纠正了我以下误判：

| 我最初认为 | Oracle 纠正 | 结论 |
| --- | --- | --- |
| `status_hfont` 热重载泄漏 | `reload_config` 中 `take()` + `DeleteObject` 正确释放了 | ❌ 我错了 |
| `url_encode` 中 `*` 未编码 | `_` 通配分支已正确编码为 `%2A` | ❌ 我错了 |
| `NORMAL_PRIORITY_CLASS` 有误 | `0x20` 是合法创建标志 | ❌ 我错了 |
| 每次 `toggle_win` 都重新解析配置 | `reload_config` 内部有 mtime 检查 | ❌ 我错了 |

Oracle 补充的关键遗漏：

| 遗漏项 | 重要性 |
| --- | --- |
| 列表项 `DrawTextW` 缺少 `DT_NOPREFIX`（P0-4） | 🔴 严重 |
| UTF-8 BOM 吞掉配置第一个区段头（P1-5） | 🟠 重要 |
| `SetForegroundWindow` 可能失败（P1-6） | 🟠 重要 |
| `SetWindowRgn` 失败时 HRGN 泄漏（P2-13） | 🟡 中等 |
| `static mut TRAY_HWND` unsound（P1-8） | 🟠 重要 |

---

## 跨领域观察

### 资源泄漏汇总

| 资源类型 | 位置 | 泄漏方式 |
| --- | --- | --- |
| 堆内存 (Box) | `main.rs` → `wndproc.rs` WM_DESTROY | `Box::into_raw` 的 AppState 未 `Box::from_raw` |
| GDI HICON | `tray.rs` | `CreateIconFromResourceEx` 后无 `DestroyIcon` |
| GDI HRGN | `state.rs` `round_win` | `SetWindowRgn` 失败时条件泄漏 |
| GDI HFONT | `wndproc.rs` WM_DESTROY | AppState 未 drop → `hfont`/`status_hfont` 未释放 |

### 静默失败路径

| 失败场景 | 结果 | 用户感知 |
| --- | --- | --- |
| config.toml 损坏 | `config.rs` 返回空列表 | 空列表，无提示 |
| config.toml 有 BOM | 第一个 section header 被吞 | 缺少分类条目，无提示 |
| `RegisterHotKey` 已占用 | `main.rs` eprintln! 报错 | 不可见（windows_subsystem） |
| `CreateProcessW` 失败 | `executor.rs` 忽略返回值 | 无反馈 |

### 双缓冲完整性

- WM_PAINT ✅ 走内存 DC 双缓冲
- VK_UP/DOWN ❌ 直接 `GetDC` 写屏，无双缓冲（在慢速机器/远程桌面上可能有闪烁）

---

## 审查结论

| 等级 | 数量 | 核心内容 |
| --- | --- | --- |
| **P0（必须修）** | 4 项 | 资源泄漏（Box 未 drop、HICON 未释放）、全局 allow 掩埋错误、列表 `&` 显示 bug |
| **P1（建议修）** | 4 项 | 配置文件兼容性（BOM、颜色 `#` 前缀）、字体解析鲁棒性、unsound 静态变量 |
| **P2（代码质量）** | 10 项 | 重复代码、Safety 文档缺失、导航键缺失、未检查返回值、HRGN 泄漏 |
| **P3（边缘项）** | 3 项 | 调试残留、命令无校验、eprintln! 不可见 |

**优先修复建议**：P0 的 4 个问题都是单行到几行修改，但影响内存安全、资源完整性和显示正确性，建议下个迭代优先处理。P1 的 BOM 问题和 `SetForegroundWindow` 失败路径影响配置文件兼容性和用户体验，也值得尽早修复。