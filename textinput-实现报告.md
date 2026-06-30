# TextInput 单行输入框实现报告

## 1. 概述

`TextInput` 是 Gua 的单行文本输入控件，支持两种显示模式：

- **左对齐模式**（`center = false`）：文本从左侧开始显示，超宽时通过 `scroll_x` 水平滚动
- **居中模式**（`center = true`）：文本在输入框内水平垂直居中，超宽时自动降级为左对齐 + 滚动

所有 Widget 通过 `widget.rs` 中的 `Widget` trait 与 `settings.rs` 集成。

---

## 2. 数据结构

```rust
pub struct TextInput {
    r: D2D_RECT_F,                          // 控件边框
    pub text: String,                        // 文本内容
    pub placeholder: String,                 // 占位符
    focused: bool,
    cursor_pos: usize,                       // 光标位置 (byte offset)
    hovered: bool,
    select_all: bool,
    pub center: bool,                        // 是否居中模式
    pub select_on_focus: bool,               // 聚焦时全选
    scroll_x: std::cell::Cell<f32>,          // 水平滚动偏移
    scroll_hold: std::cell::Cell<bool>,      // 阻止 auto-scroll（重建/鼠标释放后）
    mouse_down: bool,
    sel_start: Option<usize>,                // 选择起始 (None = 无选区)
    sel_end: usize,                          // 选择终点
    dwrite_factory: Option<IDWriteFactory>,  // 用于 on_mouse_move 的 dwrite
}
```

`scroll_x` 使用 `Cell<f32>` 是因为 `draw()` 只接受 `&self`，不能修改成员变量。

```rust
pub struct MultilineTextInput {
    r: D2D_RECT_F,                          // 控件边框
    pub text: String,                        // 文本内容
    focused: bool,
    scroll_y: std::cell::Cell<f32>,          // 垂直滚动偏移
    content_h: std::cell::Cell<f32>,         // 实际内容高度（用于滚动条）
    scroll_hold: std::cell::Cell<bool>,      // 阻止 auto-scroll
    hovered: bool,
    cursor_pos: usize,
    mouse_down: bool,
    sel_start: Option<usize>,
    sel_end: usize,
    dwrite_factory: Option<IDWriteFactory>,
}
```

`scroll_y` 和 `content_h` 使用 `Cell<f32>` 因为 `draw()` 接受 `&self`。

---

## 3. 坐标系方案

### 3.1 核心变量

| 变量 | 含义 |
|------|------|
| `box_w` | TextLayout 宽度 |
| `rel_x` | HitTestPoint 的 x 坐标（相对于 layout 左边界） |
| `sx` | 水平滚动偏移 |
| `text_r` | 左对齐时 draw_text 的矩形 |
| `full_r` | 居中对齐时 draw_text 的矩形 |

### 3.2 两种模式的坐标系

| 场景 | `use_center` | `box_w` | `rel_x` 公式 | 光标 x 公式 |
|------|-------------|---------|-------------|------------|
| 居中·文字能容纳 | `true` | `right - left` | `x - left` | `left + px` |
| 左对齐 / 居中超宽 | `false` | `right - left - 16` | `x - (left + 8) + sx` | `left + 8 + px - sx` |

### 3.3 `use_center` 判定

```rust
let use_center = if self.center {
    // create left-aligned nowrap TextFormat, measure text width
    make_tf(&res.dwrite, 14.0)
        .map(|tf| text_width(&res.dwrite, &tf, &self.text) <= (self.r.right - self.r.left))
        .unwrap_or(true)
} else {
    false
};
```

关键点：
- 使用 **左对齐** 的 `make_tf`（而非 `tf_center_nowrap`）测量宽度，因为我们要判断的只是"文本本身有多宽"
- `text_width` 内部创建 layout（宽度=10000）并读取 `widthIncludingTrailingWhitespace`
- 测量宽度与 **完整框宽**（`right - left`）比较，而非 `box_w`（窄16px）

---

## 4. 绘制流程 (`draw`)

```
1. 绘制背景 (rounded rect)
2. PushAxisAlignedClip (剪裁到 left+6 ~ right-6)
3. 计算 use_center
4. 文字绘制分支：
   a. select_all → 绘制高亮 + 文字
   b. 空文本 + placeholder → 灰字占位符
   c. use_center → tf_center_nowrap + full_r（全宽居中）
   d. else → tf_vcenter_nowrap + text_r（左对齐，可滚动）
5. Layout 创建（用于光标/选区/自动滚动）：
   a. 选区高亮（HitTestTextPosition 取两个端点 px）
   b. 光标（HitTestTextPosition 取 px）
   c. 自动滚动（仅 !use_center）
6. PopAxisAlignedClip
```

### 4.1 选区绘制

使用 `HitTestTextPosition` 分别获取选区首尾的像素偏移，然后画矩形：

```rust
let sel_l = if use_center { self.r.left + px1.min(px2) }
            else { self.r.left + 8.0 + px1.min(px2) - sx };
```

### 4.2 光标绘制

同样用 `HitTestTextPosition` 获取像素偏移，画竖线：

```rust
let cx = if use_center { self.r.left + px }
         else { self.r.left + 8.0 + px - sx };
```

### 4.3 自动滚动

仅 `!use_center` 时生效。每帧 `draw()` 检查光标（或选区远端）的像素位置：
- 若 `px < sx + 10` → 向左滚，让光标离左边缘至少10px
- 若 `px - sx > box_w2 - 10` → 向右滚，让光标离右边缘至少10px

```rust
if !self.scroll_hold.get() || self.mouse_down {
    let far = if self.mouse_down { self.sel_end }
              else { self.cursor_pos };
    let cushion = 10.0;
    if fpx < sx + cushion { sx = (fpx - cushion).max(0.0); }
    else if fpx - sx > box_w2 - cushion { sx = (fpx - box_w2 + cushion).max(0.0); }
}
```

`scroll_hold` 机制：
- 构造器、`on_mouse_up`、`on_mouse_wheel` → `scroll_hold = true`（阻止自动滚动）
- `on_mouse_down`、`on_click_with`、`on_key_down`、`on_char` → `scroll_hold = false`（恢复自动滚动）
- `draw()` **只检查不清除**

拖选期间 `far = sel_end`（始终跟踪移动端），修正了之前 `sel_end.max(cursor_pos)` 导致向左拖选时 `far` 停留在起点不动的 bug。

注意：`box_w2` 固定为 `right - left - 16`（layout 宽度），因为自动滚动只在左对齐 layout 上执行。

---

## 5. 点击定位 (`on_click_with`)

收到点击事件后：

1. 保存 `dwrite_factory` 供后续 `on_mouse_move` 使用
2. 清空选区
3. 计算 `use_center`
4. 根据 mode 确定 `box_w` 和 `rel_x`
5. `CreateTextLayout` → `HitTestPoint(rel_x, 0, ...)` → `cursor_pos`

### 关键：`rel_x` 不要 clamp

```rust
// ❌ 错误做法（v1-v3 的 bug 根因）
let _ = layout.HitTestPoint(rel_x.clamp(0.0, box_w), ...);

// ✅ 正确做法（v4 修复）
let _ = layout.HitTestPoint(rel_x, ...);
```

原因：`nowrap` 的 TextLayout 中文字可以超出 layout 宽度，`HitTestPoint(x, y)` 接受 `x > box_w`，返回最末尾字符。clamp 到 `box_w` 后，超出 `box_w` 的字符永远点击不到。

---

## 6. 拖选 (`on_mouse_move`)

流程：

```
1. 命中检测 (hovered)
2. 若 mouse_down && focused：
   a. 获取 dwrite_factory
   b. 计算 use_center
   c. 计算 box_w
   d. 边缘滚动（仅 !use_center 且文字溢出时）
   e. 计算 rel_x
   f. CreateTextLayout → HitTestPoint → sel_end
```

### 6.1 边缘滚动逻辑

```rust
if !use_center {
    let tw = make_tf(dwf, 14.0).map(|tf| text_width(dwf, &tf, &self.text)).unwrap_or(0.0);
    let edge = 16.0;
    let can_scroll_left = sx > 0.0;
    let can_scroll_right = tw > sx + box_w;
    if can_scroll_left && x < self.r.left + edge {
        sx = (sx - 4.0).max(0.0);
    } else if can_scroll_right && x > self.r.right - edge {
        sx = sx + 4.0;
    }
    self.scroll_x.set(sx);
}
```

三闸门：
- 仅 `!use_center`（不需要滚动居中的文字）
- 左滚：`sx > 0`（已经滚到右边了）
- 右滚：`text_width > sx + box_w`（右侧还有隐藏文字）
- `else if` 防止左右同时触发

---

## 7. 踩坑记录

### 坑 1：`use_center` 在 draw 和交互方法间不一致 ← 最大 Bug

**现象**：居中模式文字超宽后，点击/拖选位置与光标/选区绘制位置错位。

**根因**：`draw()` 用了 `use_center`（超宽→左对齐），但 `on_click_with` 和 `on_mouse_move` 用了 `self.center`（始终居中）。两个方法用不同 layout 做 HitTestPoint，坐标系不一致。

**修复**：所有方法统一 `use_center` 判断逻辑。

### 坑 2：`HitTestPoint` 的 rel_x 被 clamp ← 第二大 Bug

**现象**：超宽后光标固定在一个字符处，无法选中后续字符。向右拖选看到视区在动但选区不扩展。

**根因**：`rel_x.clamp(0.0, box_w)` 把所有命中限制在 layout 宽度内。`nowrap` layout 的文字实际长度超过 `box_w`，但 clamp 后 x 永远 ≤ `box_w`，HitTestPoint 返回同一字符。边缘滚动虽然 `sx` 递增，但 `rel_x = x - left - 8 + sx` 又被 clamp 截断。

**修复**：删除 clamp，直接传 `rel_x`。HitTestPoint 对 `x > box_w` 返回文字尾部，`x < 0` 返回文字开头。

### 坑 3：居中模式修改了 scroll_x

**现象**：居中模式下鼠标向右拖，文字不动但光标向左跑。

**根因**：`on_mouse_move` 无条件边缘滚动，即使 `use_center = true` 也修改 `scroll_x`。`draw()` 中居中模式不用 `scroll_x`（`full_r` 固定），但光标用了 `cx = left + px - sx` → sx 变化导致光标偏离。

**修复**：边缘滚动仅 `!use_center` 时执行。

### 坑 4：边缘滚动无条件触发

**现象**：左对齐未超宽时，鼠标到右边缘视区向右滚出现空白。

**根因**：边缘滚动没有检查文字是否真正溢出。

**修复**：添加 `can_scroll_left` 和 `can_scroll_right` 双条件闸门。

### 坑 5：居中模式 layout width 与绘制 rect 不一致

**背景**：左对齐用 `box_w = right - left - 16`（左右各8px padding），居中模式 draw_text 用 `full_r = {left, top, right, bottom}`（全宽无padding）。

但早期代码布局也用 `box_w = right - left - 16`，导致 HitTestPoint 的坐标空间比绘制空间窄16px。文本居中时偏移量不匹配。

**处理**：居中模式统一用 `box_w = right - left`（全宽），左对齐用窄版本。所有方法中 `box_w` 按 `use_center` 分叉。

### 坑 6：`scroll_x` 作为 Cell

`scroll_x` 在 `draw()` 中需要修改（自动滚动），但 `draw()` 接受 `&self`。用 `Cell<f32>` 绕过借用检查。注意 `Cell` 不是 `RefCell` —— 只能整体读写，不能借用引用。

### 坑 7：搜索框重建后光标消失（与 TextInput 无直接关系，由 settings 调用方引起）

**现象**：在搜索框中输入一个字符后，光标消失，但输入和删除功能正常。

**根因**：搜索框属于 codes tab（`s.cat == 2`），每次按键触发 `need_rebuild = true`，`build_codes_tab` 创建全新 `TextInput`，新控件 `focused = false`。settings 将 `focused_idx = Some(1)` 记录为焦点索引，但**没有调用新控件的 `set_focused(true)`**。draw 检查 `self.focused` → false → 不画光标。但键盘事件分发使用 `focused_idx` 索引而非 widget 的 `focused` 字段，所以输入/删除仍正常。

**修复**：重建前保存焦点，重建后恢复（仅同 tab 内重建，不包括切 tab）：

```rust
let restore_focus = if !was_cat_switch { s.focused_idx } else { None };
s.focused_idx = None;
// ... 重建 widgets ...
if let Some(idx) = restore_focus {
    if idx < s.widgets.len() {
        s.focused_idx = Some(idx);
        s.widgets[idx].set_focused(true);
    }
}
```

注意：v4 使用的是直接 `s.focused_idx = Some(1)` + `set_focused(true)`，这会导致切换 tab 时搜索框自动聚焦。v6 改为 `restore_focus`，只在同 tab 内才保留焦点。

### 坑 8：多行输入框纯英文异常截断换行

**现象**：`MultilineTextInput` 在一行文字不满时一切正常，但纯英文输入占满一行后，有概率从行中间某个位置突然换行，当前行尾部留空。汉字几乎不触发。

**根因**：`CreateTextLayout` 时 `SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)` — 仅在**单词边界**换行。纯英文连续字符（如 `"notepad.exe"`）被 DirectWrite 视为一个单词。当该词长度 > layout 宽度时，整个词被推到下一行，当前行尾部留空。中文字符每个字都是 Unicode 换行机会，所以无此问题。

**修复**：改用 `DWRITE_WORD_WRAPPING_CHARACTER`，在字符边界换行：

```rust
// widget.rs:965 — 改前
unsafe { let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP); }
// 改后
unsafe { let _ = tf.SetWordWrapping(DWRITE_WORD_WRAPPING_CHARACTER); }
```

副作用：英文长URL或路径会在字符边界断开，视觉上不如单词边界整齐。但对单行英文不多行文本框，这是必要的取舍。

### 坑 9：三次尝试修复拖选后视区跳回起点

**现象**：文字超宽后拖选，松开鼠标视区跳回光标（选区起点）位置。

**根因演变**：

| 尝试 | 改了什么 | 为什么失败 |
|------|---------|-----------|
| v4 | `on_mouse_up` 设 `cursor_pos = sel_end` | 光标跳到选区末尾，用户不期望 |
| v4 撤回 | `on_mouse_up` 改回原样 | 光标回到起点，auto-scroll 又把它显示出来 |
| v5 | 加 `scroll_locked: Cell<bool>`，mouse_up 锁定一帧 | draw 里清除锁了，第二帧仍然跳 |
| v6 | `scroll_hold: Cell<bool>`，改成交互时才清除 | 才真正解决 |

**最终修复（v6）**：`scroll_hold` 状态机：

```
  构造器 → scroll_hold = true
  on_mouse_up → scroll_hold = true
  on_mouse_wheel → scroll_hold = true  （多行）
  ─────────────────────────────────────
  on_mouse_down → scroll_hold = false
  on_click_with → scroll_hold = false
  on_key_down → scroll_hold = false
  on_char → scroll_hold = false
  ─────────────────────────────────────
  draw() 的 auto-scroll:
    if scroll_hold → 跳过（不清除）
    else → 执行
```

draw **只检查不清除**，所以只要用户不交互，视区永不自动跳。

### 坑 10：向左拖选视区不动

**现象**：从左向右拖选时视区跟着鼠标向右滚，但从右向左拖选时视区不动。

**根因**：auto-scroll 的 `far` 公式为 `sel_end.max(cursor_pos)`。`max` 始终取**右端**。向左拖时 `sel_end < cursor_pos`，`far = cursor_pos`（固定起点），auto-scroll 检测不到左端在移动。

**修复**：`far = sel_end`（始终跟踪移动端）。

### 坑 11：多行滚轮只能滚一点

**现象**：多行输入框用鼠标滚轮只能滚动一点点就卡住。

**根因**：滚轮设 `scroll_y` 后，下一帧 `draw()` 的 auto-scroll 接管——用 `cursor_pos` 做 `far`，如果光标在当前可见区域，auto-scroll 把 `scroll_y` 调回原位，抵消了滚轮的效果。

**修复**：`on_mouse_wheel` 设 `scroll_hold = true`，阻止 auto-scroll 干扰。

---

## 8. 注意事项（踩坑总结）

1. **坐标系统一要统一**：draw / on_click_with / on_mouse_move 必须用同一套坐标判断。将"居中→左对齐降级"的逻辑提取为公共函数可避免不一致。

2. **HitTestPoint 的 x 不需要 clamp**：DirectWrite 的 `nowrap` layout 支持 `x > box_w`。clamp 会阻止用户选中超出 layout 宽度的字符。

3. **`use_center` 用左对齐格式测量**：`make_tf`（左对齐）+ `text_width` 测量的是文本自然宽度，与居中/左对齐无关。不要用 `tf_center_nowrap` 测量。

4. **居中模式不滚动**：当 `use_center = true` 时，`scroll_x` 应保持为 0。不要在居中模式修改它。自动滚动、边缘滚动都只在 `!use_center` 时执行。

5. **Auto-scroll 只修 `draw` 不修 `on_click_with`**：点击后第一次 draw 才会执行自动滚动，所以点击后可能有一帧的光标位置不对。要完全消除这一帧延迟，需要在 `on_click_with` 中也加入同步的自动滚动逻辑。目前这帧延迟不明显，未处理。

6. **`dwrite_factory` 延迟初始化**：`on_mouse_move` 依赖 `dwrite_factory`（在 `on_click_with` 中写入）。拖选不会在第一次点击前触发，所以此方案安全。

7. **选区的 `sel_start` 在 `on_mouse_down` 中设置**，而 `on_mouse_down` 在 `on_click_with` 之后调用。所以 `on_click_with` 中 `self.sel_start = None` 是安全的——on_mouse_down 会紧接着覆盖。

8. **`scroll_hold` 只在 draw 里检查，不要在 draw 里清除**。否则 scroll_hold 只会跳一帧，下一帧继续跳。正确做法：仅在用户交互方法（`on_mouse_down` / `on_click_with` / `on_key_down` / `on_char`）中清除。`on_mouse_up` 和 `on_mouse_wheel` 设 hold。

9. **`far` 必须跟踪移动端，而非取 Max**。拖选时唯一在变的是 `sel_end`。`sel_end.max(cursor_pos)` 会把向左拖选的 `far` 锁定在 `cursor_pos`（起点），导致 auto-scroll 以为用户还在右端。

10. **`TextInput::new` 的 `cursor_pos` 应为 `text.len()`**。原始代码取 `text.len()` 是正确的——新输入框的光标应当在末尾。之前改成 `0` 导致两个问题：搜索框重建后光标固定在左端、文字从光标右面出现。配合 `scroll_hold` 首帧保护，不会触发 auto-scroll 跳转。

---

## 9. 版本历史

| 版本 | 变更 | 说明 |
|------|------|------|
| 原始代码 | `self.center` 始终居中，无边缘滚动 | 超宽时居中溢出两侧，光标可能不可见 |
| v1 | draw 中引入 `use_center`，添加边缘滚动 | 交互方法未同步 → 坐标系错位 |
| v2 | 统一三处 `use_center`，边缘滚动加闸 | 但 `HitTestPoint` clamp 未删 → 超宽后字符选不到 |
| v3 | 删除 `rel_x.clamp(0.0, box_w)` | 超宽后可正常选中全部字符 |
| v4 | 搜索框重建后恢复焦点 + 多行英文改为 CHARACTER 换行 | 光标不消失；英文逐字断行 |
| v5 | `ThreeDotsButton` 自包含弹出菜单；`scroll_locked` 尝试失败 | 三点菜单不再穿透；但 `scroll_locked` 只跳一帧 |
| v6 | `scroll_hold` 交互式清除；`far = sel_end` 单向拖选跟踪；`on_mouse_wheel` 设 hold；`TextInput::new` cursor_pos 改回 `text.len()` | 拖选后不跳回、向左拖选视区跟着走、滚轮不被 auto-scroll 抵消、重建后不跳末尾 |

---

## 10. 参考：相关函数

| 函数 | 文件:行 | 用途 |
|------|---------|------|
| `make_tf` | widget.rs:54 | 创建基本左对齐 TextFormat |
| `tf_center_nowrap` | widget.rs:1224 | 居中 + 不换行 TextFormat |
| `tf_vcenter_nowrap` | widget.rs:1234 | 垂直居中 + 不换行 TextFormat |
| `text_width` | widget.rs:78 | 测量文本宽度（CreateTextLayout + GetMetrics） |
| `cursor_from_x` | widget.rs:110 | **未使用**，用 `HitTestPoint` 从 x 映射到字符位置 |
| `TextInput::sync_scroll` | widget.rs:353 | 方向键后同步滚动偏移，与 draw 中的 auto-scroll 逻辑一致 |
| `utf16_to_byte` | widget.rs:91 | UTF-16 位置 → byte 位置 |
| `byte_to_utf16` | widget.rs:101 | byte 位置 → UTF-16 位置 |
| `MultilineTextInput` (struct) | widget.rs:857+ | 多行输入框结构 |
| `MultilineTextInput::draw` | widget.rs:1101 | 多行输入框绘制（含自动滚动 + 滚动条） |
| `MultilineTextInput::on_mouse_move` | widget.rs:911 | 多行纵向边缘滚动拖选 |
| `MultilineTextInput::on_click_with` | widget.rs:955 | 多行点击定位光标 |
| `MultilineTextInput::on_key_down` | widget.rs:998 | 多行键盘（含↑↓纵向跳行） |
| `MultilineTextInput::on_char` | widget.rs:1084 | 多行字符（`\r→\n`） |
| `MultilineTextInput::on_mouse_wheel` | widget.rs:988 | 多行滚轮滚动 |
| `ThreeDotsButton` (struct) | widget.rs:707 | 自包含三点菜单按钮 |
| `ThreeDotsButton::draw` | widget.rs:781 | ⋮ 按钮绘制 |
| `ThreeDotsButton::draw_overlay` | widget.rs:790 | 弹出菜单绘制（在所有 widget 之上） |
| `ThreeDotsButton::on_click_with` | widget.rs:751 | 点击处理（按钮 toggle + 菜单项选中） |
| `ThreeDotsButton::on_mouse_move` | widget.rs:740 | 弹出菜单 hover 追踪 |
| `Dropdown::draw_overlay` | widget.rs:1387 | 下拉菜单弹出列表绘制（参考——弹出式菜单的模板模式） |
