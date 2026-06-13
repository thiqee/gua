# KeyHop 待修问题

## 会崩/会错的

1. **[已完成] `executor.rs` URL 参数编码只转了空格** — 新增 `url_encode` 函数，对所有 URL 特殊字符做百分号编码。

2. **[已完成] `WM_ACTIVATE` 失焦处理** — 去掉 `suppress_activate` 标记，右键托盘时窗口直接失焦清空，不再保留显示。

3. **[已完成] `fill_list` 只调高度不调左右位置** — 窗口每次弹出都只改高度，如果之前被手动挪过，不会回到配置的位置。且每次调用都执行 `SetWindowPos` + `round_win`（`SetWindowRgn`），即使搜索结果数没变也重复执行，浪费性能。

4. **[已完成] `seh_filter` 异常回调写 `crash.log`** — 崩溃回调里做文件 IO，堆可能已经坏了，写文件本身可能再崩；而且返回 1 等于告诉系统"我处理好了你别管"，但程序已经不可恢复了。

## 别扭但不崩的

6. **[已完成] 配置解析 `split_once('=')`** — 改为 `find('=')` 取第一个 `=`，去掉 `trim_matches('"')`。

7. **[暂缓] `entry_type` 先判 `.exe` 再判 URL** — 如果一条记录同时满足，分类标成"程序"而不是"网址"，实际碰不上，暂不处理。

8. **[不处理] `SetProcessDPIAware` 过时** — windows crate 0.62 中没有导出新 API `SetProcessDpiAwareness`，旧 API 在 Win10/11 上正常工作。保持现状。

## 代码脏

9. **[不处理] `#![allow(unused_must_use)]`** — Win32 API 编程中大量调用返回 BOOL/Result，失败时程序无法恢复，逐行加 `let _ =` 只会增加 44 处代码噪声，不会提升安全性。保持现状。

10. **[已完成] `main.rs` 拆分为 5 个文件** — 按职责拆为 state、draw、window、wndproc，main.rs 仅保留入口和 main()。

11. **[暂缓] `RegisterHotKey` / `SetFocus` 重复 extern 声明** — windows crate 已经导出了，自己又手写一遍，类型用 `u32` 而非枚举。需要确认 windows crate 中正确函数名后再处理。

12. **[已完成] `WM_IME_COMPOSITION` 中 `lp.0 as u32` 截断** — `lp.0` 是 `isize`（64 位下 8 字节），转 `usize` 避免截断。
