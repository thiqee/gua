# KeyHop 待修问题

## 会崩/会错的

1. **`executor.rs` URL 参数编码只转了空格** — `#` `&` 等字符没转，cmd 启动时 URL 被截断。

2. **`WM_ACTIVATE` 失焦处理** — 点托盘菜单时窗口失焦会触发隐藏清空，当前用 `suppress_activate` 标记绕，但菜单关闭时还是可能闪退清空。

3. **`fill_list` 只调高度不调左右位置** — 窗口每次弹出都只改高度，如果之前被手动挪过，不会回到配置的位置。

4. **`seh_filter` 异常回调写 `crash.log`** — 崩溃回调里做文件 IO，堆可能已经坏了，写文件本身可能再崩；而且返回 1 等于告诉系统"我处理好了你别管"，但程序已经不可恢复了。

5. **`rebuild_font` 直接删旧字体** — 如果删的时候 DC 里还选着这个字体，GDI 崩。

## 别扭但不崩的

6. **配置解析 `split_once('=')`** — 值里有 `=` 会被切错；`trim_matches('"')` 会吃掉值里正常的双引号；重复 key 不报错。

7. **`entry_type` 先判 `.exe` 再判 URL** — 如果一条记录同时满足，分类标成"程序"而不是"网址"，实际碰不上。

8. **`SetProcessDPIAware` 过时** — 推荐用 `SetProcessDpiAwareness`，但 Win10/11 上还能跑。

## 代码脏

9. **`#![allow(unused_must_use)]`** — `main.rs` 和 `tray.rs` 都开了，所有 `Result` 返回值全扔了不处理。

10. **`main.rs` 640 行塞了全部逻辑** — 窗口过程、绘制、IME、配置、托盘全部揉一起，没法单元测试。

11. **GDI+ 函数手写导入** — 没用 `#[link(name = "gdiplus")]`，靠 windows crate 内部链接，不稳定。

12. **`RegisterHotKey` / `SetFocus` 重复 extern 声明** — windows crate 已经导出了，自己又手写一遍，类型用 `u32` 而非枚举。
