# Gua 渲染架构重构方案 —— DComp + D3D11 + Direct2D

> 编写日期：2026-06-26
> 目标：用现代硬件加速渲染管线完全替代当前 GDI + GDI+ 混合方案，解决窗口圆角锯齿问题，为后续功能打好基础。

---

## 目录

1. [现状与问题](#1-现状与问题)
2. [目标架构](#2-目标架构)
3. [技术选型详解](#3-技术选型详解)
4. [文件改动清单](#4-文件改动清单)
5. [核心数据流](#5-核心数据流)
6. [关键实现细节](#6-关键实现细节)
7. [各文件具体改动](#7-各文件具体改动)
8. [注意事项与陷阱](#8-注意事项与陷阱)
9. [附：相关 API 参考](#9-附相关-api-参考)

---

## 1. 现状与问题

### 当前渲染管线

```
GDI  Region（CreateRoundRectRgn + SetWindowRgn）→ 窗口圆角裁剪（锯齿严重）
    │
    ├── GDI  FillRect（theme_color）→ 窗口背景填色
    │
    ├── GDI+ fill_round_rect → 输入框圆角背景（反走样，平滑）
    │
    ├── GDI  DrawTextW → 输入框文字 + 列表文字 + 状态栏文字
    │
    └── GDI  BitBlt（内存 DC → 屏幕）
```

### 三个问题

1. **窗口圆角锯齿**——SetWindowRgn + CreateRoundRectRgn 用 GDI Region 硬切窗口，边缘没有半透明过渡，产生锯齿。
2. **GDI/GDI+ 混合绘制**——同一种 UI 用两种技术，输入框圆角用 GDI+ 反走样，窗口圆角却只能用 GDI Region 硬切。
3. **CPU 软渲染**——GDI/GDI+ 都是 CPU 渲染，虽然对于 Gua 来说性能不是瓶颈，但架构老旧。
4. **GDI 文字像素对齐**——在高 DPI 下字符间距可能不均匀。

### 之前被否决的方案

- **GDI+ + UpdateLayeredWindow**：ID2D1DCRenderTarget 让 D2D 退化为软渲染，失去硬件加速意义。

---

## 2. 目标架构

```
WS_EX_NOREDIRECTIONBITMAP 窗口
    │
    └── DComp Visual Tree（可选，不兼容时自动回退）
            │
            └── DXGI SwapChain（IDXGISwapChain1）
                    │
                    └── D2D DeviceContext（ID2D1DeviceContext）
                            │
                            ├── D2D FillRoundedRectangle（窗口圆角背景）
                            ├── D2D FillRoundedRectangle（输入框圆角、高亮）
                            ├── DWrite DrawTextLayout（所有文字）
                            └── SwapChain Present(0, DXGI_PRESENT_ALLOW_TEARING)
```

**关键架构决策：**

| 决策点 | 选择 | 理由 |
|---|---|---|
| 窗口样式 | WS_EX_NOREDIRECTIONBITMAP | 关闭 GDI 重定向表面，SwapChain 直接提供窗口内容 |
| 3D 设备 | D3D11 | 成熟稳定，D2D 1.1 原生绑定 D3D11 设备 |
| 2D 渲染 | D2D 1.1 DeviceContext | 硬件加速、反走样圆角原生支持、与 D3D11 共享设备 |
| 文字渲染 | DirectWrite | 子像素定位、高 DPI 清晰、与 D2D 深度集成 |
| 合成方式 | DirectComposition（可选） | 提供 visual clip 双重保障，但不作为硬依赖 |
| 帧提交 | DXGI SwapChain Present(0, ALLOW_TEARING) | 最低延迟，输入响应优先于帧率 |

---

## 3. 技术选型详解

### 3.1 WS_EX_NOREDIRECTIONBITMAP 为什么比 WS_EX_LAYERED 好

| | WS_EX_LAYERED（旧） | WS_EX_NOREDIRECTIONBITMAP（新） |
|---|---|---|
| 内容来源 | GDI 重定向表面 | DXGI SwapChain 直接提供 |
| DWM 合成方式 | 从 GDI 表面拷贝到 DWM 表面（慢） | 直接引用 GPU 表面（零拷贝） |
| 性能 | 差（每帧还多一次 GDI→DX 拷贝） | 好（原生 DXGI 翻转） |
| 逐像素透明 | 支持 | 支持（SwapChain alpha channel） |
| 兼容性 | WinXP+ | Win8+ |

### 3.2 为什么需要 D3D11 才能用 D2D 硬件加速

D2D 1.1 的 ID2D1DeviceContext 需要绑定到 IDXGIDevice（即 D3D11 设备）才能获得 GPU 加速：

```
ID3D11Device → IDXGIDevice → ID2D1Device → ID2D1DeviceContext
```

### 3.3 DirectComposition 的作用

DComp 构建 visual 树，通过 SetClip 提供圆角裁剪并在合成层实现鼠标事件穿透，同时支持硬件加速动画。

**DComp 回退：** Windows 7 上不可用，回退到纯 SwapChain 模式。圆角视觉正常（D2D 保持），仅透明区域的鼠标事件不再穿透。由于 Gua 是键盘操作的弹出面板，此差异影响很小。

---

## 4. 文件改动清单

| # | 文件 | 改动内容 | 程度 |
|---|---|---|---|
| 1 | Cargo.toml | feature 从 Gdi/GdiPlus 改为 D3D11+DXGI+D2D+DWrite+DComp+COM | 小 |
| 2 | main.rs | 移除 GDI+，新增 CoInit/SwapChain(D2D/DWrite)/DComp/MakeWindowAssociation/TEARING 检测 | 中 |
| 3 | state.rs | HFONT → IDWriteTextFormat；新增画刷/TextLayout 缓存、device_recover_attempts、renderer 空指针检查 | 中 |
| 4 | draw.rs | 整套重写——GDI+ → D2D，DrawTextW → DrawTextLayout（使用缓存 TextLayout） | 大 |
| 5 | wndproc.rs | WM_PAINT 重写——D2D BeginDraw+EndDraw+Present；设备丢失检测+递归保护 | 大 |
| 6 | window.rs | 移除 round_win；rebuild 改为 rebuild_text_format+brushes+text_layouts；唤出强制重绘；recreate_renderer 完整+空指针保护 | 中 |
| — | executor.rs / config.rs / tray.rs / plugin.rs | 不动 | 无 |

---

## 5. 核心数据流

### 5.1 初始化流程

```
main()
    │
    ├── 1. 单例检查
    │
    ├── 2. CoInitializeEx(None, COINIT_APARTMENTTHREADED)
    │     └── 检查返回值：S_OK(0)=首次需配对CoUninitialize / S_FALSE(1)=已初始化过不配对 / 其他=致命错误
    │
    ├── 3. 加载配置（不动）
    │
    ├── 4. 创建窗口（WS_EX_NOREDIRECTIONBITMAP + WS_POPUP）
    │
    ├── 5. D3D11CreateDevice(HARDWARE, BGRA_SUPPORT)
    │
    ├── 6. 创建 DXGI SwapChain
    │     ├── CreateSwapChainForHwnd(ALPHA_MODE_PREMULTIPLIED, FLIP_DISCARD, 2 buffers)
    │     ├── Flags 必须包含 DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
    │     └── MakeWindowAssociation(hwnd, MWA_NO_ALT_ENTER)
    │
    ├── 7. CheckFeatureSupport(ALLOW_TEARING) → 缓存 supports_tearing
    │
    ├── 8. D2D1CreateFactory → CreateDevice(dxgi_device) → CreateDeviceContext
    │     └── CreateBitmapFromDxgiSurface(back_buffer) → SetTarget
    │
    ├── 9. DWriteCreateFactory → CreateTextFormat
    │
    ├── 10. [可选] DComp → Visual → SetContent → SetClip → Commit
    │
    ├── 11. 注册热键、托盘图标（不动）
    │
    ├── 12. 设置 AppState
    │     ├── 创建 IDWriteTextFormat、ID2D1SolidColorBrush（缓存）
    │     └── device_recover_attempts=0，renderer 非空
    │
    └── 13. 消息循环（不动）
```

退出时：drop AppState(画刷/TextFormat/TextLayout) → drop GuaRenderer → (仅 CoInit 返回 S_OK 时) CoUninitialize()。

### 5.2 渲染流程（每帧）

```
WM_PAINT
    │
    ├── 0. if device_recover_attempts > 0 || renderer.is_null():
    │       EndPaint; return（防递归+防悬垂）
    │
    ├── 1. BeginDraw() → Clear(透明) → 窗口圆角背景 → 输入框圆角 → 输入框文字
    │
    ├── 2. [循环] 列表项（缓存 TextLayout）→ 状态栏（缓存 TextLayout）
    │
    ├── 3. EndDraw() → 检测设备丢失
    │     └── 丢失 → device_recover_attempts=1 → recreate_renderer → 成功则重置并 RedrawWindow
    │
    └── 4. Present(0, flags) → 检测设备丢失（同上）
```

### 5.3 WM_SIZE → ResizeBuffers + target 重建（参考 8.9）

### 5.4 WM_DPICHANGED → rebuild_text_format + rebuild_text_layouts

---

## 6. 关键实现细节

### 6.1 SwapChain 创建参数

```rust
let desc = DXGI_SWAP_CHAIN_DESC1 {
    Width: width as u32,
    Height: height as u32,
    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
    Stereo: false.into(),
    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
    BufferCount: 2,
    Scaling: DXGI_SCALING_STRETCH,
    SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
    AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
    Flags: DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,   // 必须！不然后续 Present(ALLOW_TEARING) 失败
};
```

**微软文档硬性要求：** 使用 DXGI_PRESENT_ALLOW_TEARING 的前提是创建时 Flags 包含了 DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING。

### 6.2 Present：SyncInterval=0 + ALLOW_TEARING

```rust
let supports_tearing = {
    let mut feature = DXGI_FEATURE_PRESENT_ALLOW_TEARING::default();
    dxgi_factory.CheckFeatureSupport(DXGI_FEATURE_PRESENT_ALLOW_TEARING, &mut feature)
        .is_ok() && feature.0 != 0
};
let flags = if supports_tearing { DXGI_PRESENT_ALLOW_TEARING } else { 0 };
swap_chain.Present(0, flags)?;
```

效率工具，输入延迟比帧率重要。Gua 按需渲染，不会出现持续可见撕裂。

### 6.3 D2D Target Bitmap

```rust
let props = D2D1_BITMAP_PROPERTIES1 {
    pixelFormat: D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    },
    dpiX: 96.0, dpiY: 96.0,
    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    colorContext: None,
};
let target = d2d_context.CreateBitmapFromDxgiSurface(&back_buffer, &props)?;
d2d_context.SetTarget(&target);
```

### 6.4 窗口圆角背景

```rust
let rounded_rect = D2D1_ROUNDED_RECT {
    rect: D2D1_RECT_F { left: 0.0, top: 0.0, right: w as f32, bottom: h as f32 },
    radiusX: corner as f32,
    radiusY: corner as f32,
};
d2d_context.FillRoundedRectangle(&rounded_rect, &theme_brush);
```

### 6.5 DComp Visual

```rust
let clip = dcomp_device.CreateRectangleClip()?;
clip.SetLeft(0.0)?; clip.SetTop(0.0)?;
clip.SetRight(w as f32)?; clip.SetBottom(h as f32)?;
clip.SetTopLeftRadiusX(corner as f32)?; clip.SetTopLeftRadiusY(corner as f32)?;
clip.SetTopRightRadiusX(corner as f32)?; clip.SetTopRightRadiusY(corner as f32)?;
clip.SetBottomLeftRadiusX(corner as f32)?; clip.SetBottomLeftRadiusY(corner as f32)?;
clip.SetBottomRightRadiusX(corner as f32)?; clip.SetBottomRightRadiusY(corner as f32)?;
visual.SetClip(&clip)?;
```

### 6.6 画刷颜色更新用 SetColor，不重建

```rust
// reload_config 中颜色变化时：
theme_brush.SetColor(&color_to_d2d(s.theme_color, s.opacity as f32 / 255.0));
accent_brush.SetColor(&color_to_d2d(s.accent_color, 1.0));
text_brush.SetColor(&color_to_d2d(s.text_color, 1.0));
```

### 6.7 TextLayout 按需创建与缓存

AppState 缓存：item_text_layouts: Vec<Option<IDWriteTextLayout>>，status_text_layout: Option<IDWriteTextLayout>。

| 触发事件 | 重建范围 |
|---|---|
| fill_list（筛选变化） | 全部 item_text_layouts |
| reload_config（字体/字号变化） | 全部 |
| DPI 变化 | 全部 |
| WM_PAINT 中 | 只 draw，不 create |

---

## 7. 各文件具体改动

### 7.1 Cargo.toml

```diff
 windows = { version = "0.62", features = [
     "Win32_Foundation",
     "Win32_UI_WindowsAndMessaging",
     "Win32_UI_Shell",
-    "Win32_Graphics_Gdi",
-    "Win32_Graphics_GdiPlus",
+    "Win32_Graphics_Direct3D11",
+    "Win32_Graphics_Dxgi",
+    "Win32_Graphics_Direct2D",
+    "Win32_Graphics_Direct2D_Common",
+    "Win32_Graphics_DirectWrite",
+    "Win32_Graphics_DirectComposition",
+    "Win32_System_Com",
+    "Win32_Graphics_Gdi",               // 保留：tray + caret
     "Win32_System_LibraryLoader",
     "Win32_System_Threading",
     "Win32_Security",
 ] }
```

### 7.2 main.rs

移除：GDI+、load_private_fonts、SetWindowRgn、round_win、SetLayeredWindowAttributes。

新增 GuaRenderer：
```rust
pub struct GuaRenderer {
    pub d3d_device: ID3D11Device,
    pub d3d_context: ID3D11DeviceContext,
    pub dxgi_factory: IDXGIFactory2,
    pub swap_chain: IDXGISwapChain1,
    pub supports_tearing: bool,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    pub d2d_context: ID2D1DeviceContext,
    pub dwrite_factory: IDWriteFactory,
    pub dcomp_device: Option<IDCompositionDevice>,
}
```

初始化顺序：
```
0. CoInitializeEx(None, COINIT_APARTMENTTHREADED)
   检查返回值：S_OK→com_initialized=true / S_FALSE→已初始化不配对 / 其他→致命退出
1. D3D11CreateDevice(HARDWARE, BGRA_SUPPORT)
2. IDXGIDevice → GetAdapter → GetParent<IDXGIFactory2>
3. CreateSwapChainForHwnd（Flags 含 DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING）
4. MakeWindowAssociation(hwnd, MWA_NO_WINDOW_CHANGES | MWA_NO_ALT_ENTER)
5. CheckFeatureSupport(ALLOW_TEARING)→cache supports_tearing
6. D2D1CreateFactory → CreateDevice → CreateDeviceContext
7. GetBuffer(0) → CreateBitmapFromDxgiSurface → SetTarget
8. DWriteCreateFactory → CreateTextFormat
9. [可选] DComp → Visual → SetContent → SetClip → Commit
```

窗口样式：
```diff
- WS_EX_TOOLWINDOW | (always_on_top ? WS_EX_TOPMOST : 0)
+ WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP | (always_on_top ? WS_EX_TOPMOST : 0)
```

WS_POPUP（当前已是）。

退出：drop AppState → drop GuaRenderer → (com_initialized) CoUninitialize()。

### 7.3 state.rs

AppState 字段：
```rust
pub struct AppState {
    // 不变字段：entries, filter, input_text, cursor_pos, ...
    pub text_format: Option<IDWriteTextFormat>,
    pub status_text_format: Option<IDWriteTextFormat>,
    pub theme_brush: Option<ID2D1SolidColorBrush>,
    pub input_bg_brush: Option<ID2D1SolidColorBrush>,
    pub accent_brush: Option<ID2D1SolidColorBrush>,
    pub text_brush: Option<ID2D1SolidColorBrush>,
    pub item_text_layouts: Vec<Option<IDWriteTextLayout>>,
    pub status_text_layout: Option<IDWriteTextLayout>,
    pub renderer: *mut GuaRenderer,
    pub device_recover_attempts: u32,
    pub theme_color: u32, pub input_bg_color: u32,
    pub accent_color: u32, pub text_color: u32, pub opacity: u8,
    pub width: i32, pub item_h: i32, pub eh: i32, pub dpi: i32,
}
```

移除：make_font_with、font_px、status_bar_h、win_h、round_win、load_private_fonts、colorref。

新增：
```rust
pub fn color_to_d2d(rgb: u32, alpha: f32) -> D2D1_COLOR_F { /* ... */ }

/// 安全访问 renderer，自动空指针检查
pub fn gua_renderer(s: &AppState) -> Option<&GuaRenderer> {
    if s.renderer.is_null() { None } else { Some(unsafe { &*s.renderer }) }
}
pub fn gua_renderer_mut(s: &mut AppState) -> Option<&mut GuaRenderer> {
    if s.renderer.is_null() { None } else { Some(unsafe { &mut *s.renderer }) }
}
```

### 7.4 draw.rs —— 整套重写

所有 GDI/GDI+ 函数替换为 D2D/DWrite，使用缓存的 TextLayout。
新增 rebuild_text_layouts(s)、rebuild_item_layout(s, index)。

### 7.5 wndproc.rs

WM_PAINT：
```
0. if device_recover_attempts > 0 || renderer.is_null(): EndPaint; return
1. BeginDraw() → Clear → 窗口圆角 → 输入框 → 输入框文字
2. 列表(缓存的 TextLayout) → 状态栏(缓存的 TextLayout)
3. EndDraw() → check_and_recover()
4. Present(0, flags) → check_and_recover()
5. EndPaint
```

设备丢失检测：
```rust
fn check_and_recover(hr: HRESULT, s: &mut AppState, h: HWND, ps: &PAINTSTRUCT) -> bool {
    if s.device_recover_attempts > 0 { return true; }
    if hr == D2DERR_RECREATE_TARGET || hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET {
        s.device_recover_attempts = 1;
        if recreate_renderer(s, h) {
            s.device_recover_attempts = 0;
            RedrawWindow(h, None, None, RDW_INVALIDATE | RDW_UPDATENOW);
        }
        EndPaint(h, ps);
        return true;
    }
    false
}
```

### 7.6 window.rs

**recreate_renderer（从 D3D11 开始重建，含空指针保护）：**
```rust
pub unsafe fn recreate_renderer(s: &mut AppState, h: HWND) -> bool {
    // 1. 释放依赖旧 Renderer 的资源
    s.theme_brush = None;
    s.input_bg_brush = None;
    s.accent_brush = None;
    s.text_brush = None;
    s.text_format = None;
    s.status_text_format = None;
    s.item_text_layouts.clear();
    s.status_text_layout = None;

    // 2. 释放旧的 Renderer
    if !s.renderer.is_null() {
        drop(Box::from_raw(s.renderer));
    }

    // 3. 重建（从 D3D11CreateDevice 开始）
    match create_renderer(h, s) {
        Ok(r) => {
            s.renderer = Box::into_raw(Box::new(r));
            create_and_cache_brushes(s);
            rebuild_text_format(s);
            rebuild_text_layouts(s);
            true
        }
        Err(_) => {
            s.renderer = ptr::null_mut();   // ← 关键：显式置空，防止悬垂指针
            false
        }
    }
}
```

**所有访问 renderer 的地方必须经过 gua_renderer() 检查：**
```rust
// toggle_win、hide_clear 等中访问 renderer 前：
let r = match gua_renderer_mut(s) {
    Some(r) => r,
    None => return,   // 设备未就绪，跳过
};
```

**fill_list()：** 移除 round_win，筛选变化后 rebuild_text_layouts。
**reload_config()：** 颜色用 SetColor；字体变化重建 TextFormat+TextLayout；移除 SetLayeredWindowAttributes。
**rebuild_text_format：** DWrite CreateTextFormat。
**toggle_win()：** ShowWindow 后立即 RedrawWindow(UPDATENOW)。

---

## 8. 注意事项与陷阱

### 8.1 COM 库初始化（必须）

```rust
let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
if hr.0 != 0 && hr.0 != 1 {
    return Err(/* 致命错误：RPC_E_CHANGED_MODE 等 */);
}
let com_initialized = (hr.0 == 0);  // S_OK 才需配对 CoUninitialize
```

S_FALSE(1) 表示本线程已初始化过 COM，不应调用 CoUninitialize。

### 8.2 禁用 DXGI Alt+Enter 拦截（必须）

```rust
dxgi_factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_WINDOW_CHANGES | DXGI_MWA_NO_ALT_ENTER)?;
```

### 8.3 设备丢失 + 递归重绘保护（必须）

device_recover_attempts：WM_PAINT 入口 >0 时跳过渲染。重建成功重置为 0，失败保持 >0。即使重建失败，WM_PAINT 静默跳过，不会栈溢出。

### 8.4 窗口唤出强制重绘（必须）

```rust
ShowWindow(h, SW_SHOW);
RedrawWindow(h, None, None, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
```

### 8.5 窗口样式必须用 WS_POPUP

### 8.6 线程安全（COM 对象 !Send+!Sync，全在主线程）

### 8.7 悬垂指针预防（必须）

recreate_renderer 的 Err 分支必须设置 s.renderer = ptr::null_mut()。所有访问 renderer 的地方必须用 gua_renderer() 辅助函数，先检查空指针。

### 8.8 Present=0 + ALLOW_TEARING

SwapChain 创建时 Flags 必须包含 DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING，否则 Present(ALLOW_TEARING) 失败。

### 8.9 SwapChain Resize 顺序

### 8.10 画刷更新用 SetColor，不重建

### 8.11 输入光标不动

### 8.12 IME 输入法

### 8.13 DComp 回退鼠标事件（已知限制）

### 8.14 透明度处理

### 8.15 文本度量

### 8.16 窗口定位

### 8.17 自定义字体取消

---

## 9. 附：相关 API 参考

| API | feature |
|---|---|
| CoInitializeEx | Win32_System_Com |
| D3D11CreateDevice | Win32_Graphics_Direct3D11 |
| CreateDXGIFactory2 / MakeWindowAssociation / CheckFeatureSupport | Win32_Graphics_Dxgi |
| Present / DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING | Win32_Graphics_Dxgi |
| D2D1CreateFactory / ID2D1SolidColorBrush::SetColor | Win32_Graphics_Direct2D |
| DWriteCreateFactory / IDWriteTextFormat / IDWriteTextLayout | Win32_Graphics_DirectWrite |
| DCompositionCreateDevice2 | Win32_Graphics_DirectComposition |

---

## 实施建议

| 阶段 | 内容 | 文件 |
|---|---|---|
| 1. 搭架子 | Cargo.toml；main.rs 实现 CoInit(S_OK/S_FALSE)、D3D11、SwapChain(含 ALLOW_TEARING Flag)、MakeWindowAssociation、TEARING 检测、D2D、DWrite、DComp | Cargo.toml, main.rs |
| 2. 窗口背景 | draw.rs D2D FillRoundedRectangle；画刷缓存+SetColor；wndproc.rs WM_PAINT；gua_renderer() 安全访问 | draw.rs, wndproc.rs |
| 3. 文字渲染 | DWrite 替换 GDI DrawTextW；TextLayout 按需创建+缓存 | draw.rs, state.rs |
| 4. 消息闭环 | WM_SIZE/DPICHANGED；window.rs toggle_win 强制重绘+recreate_renderer(6步顺序+Err置空)+设备丢失+递归保护 | wndproc.rs, window.rs |
| 5. 清理收尾 | 移除全部 GDI+；测试 DComp fallback、设备丢失重建、多 DPI | main.rs, state.rs |
