# Gua 内存管理最终方案（v3，已合并审核修正）

## 核心策略

不再尝试"硬上限"、"延迟换出"等复杂方案。回到被验证最有效的简单流程：

```
隐藏时：
  设 VeryLow 优先级 → SetProcessWorkingSetSize(-1,-1) → 不恢复

显示时：
  恢复 Normal 优先级（为下一次隐藏时的动态标记做准备）
```

## 改动概览

| 文件 | 改什么 |
|------|--------|
| `src/main.rs` | 删掉所有内存相关残余，恢复原始代码 |
| `src/state.rs` | 删 `QUOTA_LIMITS` + `working`/`mem_ceiling`/`mem_ready` |
| `src/window.rs` | `hide_clear` 设 VeryLow → trim → 不恢复；`toggle_win` 恢复 Normal |
| `src/wndproc.rs` | 删 DISABLE/ENABLE；`WM_POWERBROADCAST` 设 VeryLow → trim |

## 各文件详细改动

### 1. `src/main.rs`

删掉所有标定残余：
- `use std::mem;`
- `use windows::Win32::System::ProcessStatus::GetProcessMemoryInfo;`
- `use windows::Win32::System::Threading::GetCurrentProcess;`
- `PROCESS_MEMORY_COUNTERS_EX` 整个结构体
- 第二个 `#[link(name = "kernel32")]` extern 块（`SetProcessWorkingSetSize` + `SetProcessWorkingSetSizeEx`）
- 整个标定代码段
- AppState 构造中的 `working: false, mem_ceiling: ceiling, mem_ready: true`

最后结果：`main.rs` 只保留原始代码，无任何内存管理残余。

### 2. `src/state.rs`

删常量：
- `QUOTA_LIMITS_HARDWS_MAX_ENABLE: u32 = 4`
- `QUOTA_LIMITS_HARDWS_MAX_DISABLE: u32 = 8`

从 AppState 删字段：
- `working: bool`
- `mem_ceiling: usize`
- `mem_ready: bool`

### 3. `src/window.rs`

**FFI 块**替换为：
```rust
#[link(name = "kernel32")]
extern "system" {
    fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> i32;
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const MemPrio, size: u32) -> i32;
}
```

**加结构体和常量**：
```rust
#[repr(C)]
struct MemPrio { priority: u32 }
const PROCESS_MEMORY_PRIORITY: i32 = 0;
const MEM_PRIO_VERY_LOW: u32 = 1;
const MEM_PRIO_NORMAL: u32 = 5;
```

**加 import**：
```rust
use windows::Win32::System::Threading::GetCurrentProcess;
```

**`hide_clear` 末尾**（ShowWindow 之后）：
```rust
// 内存优先级降到最低，立即换出，保持低优先级让系统持续修剪
let hp = GetCurrentProcess();
let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio, std::mem::size_of::<MemPrio>() as u32);
let _ = SetProcessWorkingSetSize(hp, usize::MAX, usize::MAX);
```

**`toggle_win` 显示分支开头**：
```rust
if s.visible {
    hide_clear(h, s);
} else {
    // 恢复 Normal 优先级（为下一次隐藏时的动态标记做准备）
    let hp = GetCurrentProcess();
    let prio = MemPrio { priority: MEM_PRIO_NORMAL };
    let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio, std::mem::size_of::<MemPrio>() as u32);
    // 原有显示逻辑不变...
```

### 4. `src/wndproc.rs`

**FFI 块**替换为（同 window.rs）：
```rust
#[link(name = "kernel32")]
extern "system" {
    fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> i32;
    fn SetProcessInformation(h: HANDLE, class: i32, info: *const MemPrio, size: u32) -> i32;
}
```

**加结构体和常量**（同 window.rs）：
```rust
#[repr(C)]
struct MemPrio { priority: u32 }
// 常量已通过 use crate::state::* 引入
// 等等——MEM_PRIO 常量不在 state.rs。需在此定义或加 state.rs。
```

注意：`MEM_PRIO_VERY_LOW`、`MEM_PRIO_NORMAL`、`PROCESS_MEMORY_PRIORITY` 以及 `MemPrio` 结构体如果在两个文件用，要么放到 `state.rs` 统一导出，要么各文件自己定义。

**选择**：放到 `state.rs`（同 QUOTA_LIMITS 原来的位置），避免重复。

**`WM_HOTKEY` 分支**：
删掉 DISABLE 和 `working = true`，只保留 `toggle_win`。

**`WM_POWERBROADCAST` 分支**：
```rust
WM_POWERBROADCAST => {
    let evt = wp.0 as u32;
    if evt == PBT_APMRESUMESUSPEND || evt == PBT_APMRESUMEAUTOMATIC {
        let hp = GetCurrentProcess();
        let prio = MemPrio { priority: MEM_PRIO_VERY_LOW };
        let _ = SetProcessInformation(hp, PROCESS_MEMORY_PRIORITY, &prio, std::mem::size_of::<MemPrio>() as u32);
        let _ = SetProcessWorkingSetSize(hp, usize::MAX, usize::MAX);
    }
    return LRESULT(0);
}
```

## 执行时序

```
启动 → 初始化 → 消息循环（无任何内存操作）

热键按下 → toggle_win → 恢复 Normal → 窗口弹出 → 使用（内存自由膨胀）

执行/ESC/失焦 → hide_clear:
  清空状态 → ShowWindow(SW_HIDE)
  → 设 VeryLow     ← 标记所有页为低优先级
  → trim           ← 立即换出到 ~1.2MB
  → 不恢复 Normal  ← 让系统持续修剪，加速到 0.5MB

系统休眠唤醒 → WM_POWERBROADCAST:
  → 设 VeryLow + trim（和 hide_clear 一致）
```

## 和审核意见的对应

| 审核点 | 处理 |
|--------|------|
| HeapCompact 危险 | ✅ 移除 |
| TRIM_MSG 不可靠 | ✅ 移除 |
| toggle_win 恢复 Normal | ✅ 添加 |
| 不能保证 0.5 | ✅ 目标修正为"瞬间 ~1.2MB，加速到 0.5" |
| shrink_to_fit 无贡献 | ✅ 移除 |
