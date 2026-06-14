# Gua

Windows 平台快捷键启动器。按热键弹出搜索框，输入识别码快速打开网址、程序、文件或文件夹。

支持自定义热键、搜索引擎、黑名单、私有字体、IME 中文输入、配置热重载。

---

## 功能

- **热键呼出**：默认 `Ctrl+J`，可自定义任意修饰键组合
- **前缀搜索**：输入识别码实时过滤，匹配条目即选即开
- **搜索引擎**：`gg 关键词` 自动拼接 URL 搜索
- **黑名单**：指定程序在前台时热键不响应，防止游戏误触
- **私有字体**：字体文件放 `fonts/` 目录即可使用，免安装
- **光标操作**：支持 `←→` 移动、`Backspace`、`Delete`，支持中文输入
- **系统托盘**：后台运行，右键菜单退出

---

## 快速开始

1. 运行 `gua.exe`，程序自动隐藏到系统托盘
2. 按热键（默认 `Ctrl+J`）呼出搜索框
3. 输入识别码（如 `gh`），按 `Enter` 执行

首次运行需要先创建 `config.toml`（见下方配置说明），否则没有条目可启动。

---

## 安装

从 [Releases](https://github.com/你的用户名/gua/releases) 下载最新版本，解压即可运行，无需安装。

推荐目录结构：

```
gua/
├── gua.exe
├── config.toml      # 配置文件（需自行创建）
├── gua.ico          # 托盘图标（可选，不配则用默认）
└── fonts/           # 字体目录（可选）
    ├── LXGWWenKai-Regular.ttf
    └── SmileySans-Oblique.ttf
```

---

## 配置

程序启动时读取同目录下的 `config.toml`。修改后按热键或点托盘图标即可热重载，无需重启。

### 完整配置示例

将以下内容保存为 `config.toml`，放在 `gua.exe` 同目录：

```toml
_hotkey = Alt+Space          # 热键，格式：修饰键+按键
_blacklist =                 # 黑名单，逗号分隔 exe 名
_font = Segoe UI             # 字体家族名称
_font_size = 18              # 字号

_width = 500                 # 窗口宽度（像素）
_round_corner = 12           # 窗口圆角大小
_always_on_top = true        # 窗口置顶
_opacity = 255               # 透明度 0~255
_max_results = 8             # 列表最多显示条数
_case_sensitive = true       # 匹配是否区分大小写
_hide_on_focus_loss = true   # 失去焦点自动隐藏

_theme_color = 1E1E1E        # 窗口背景色（RGB 十六进制）
_input_bg_color = 2A2A2A     # 输入框背景色
_accent_color = 4A6FA5       # 选中项高亮色
_text_color = CCCCCC         # 文字颜色

_panel_position_x = 50       # 面板水平位置 0~100
_panel_position_y = 50       # 面板垂直位置 0~100

# ─── 网址 ───
[网址]
gh = https://github.com      # 输入 gh 回车打开 GitHub
b23 = https://www.bilibili.com

# ─── 程序 ───
[程序]
calc = C:\Windows\System32\calc.exe  |  计算器

# ─── 搜索引擎 ───
[搜索引擎]
gg = https://www.google.com/search?q=  |  Google 搜索
bd = https://www.baidu.com/s?wd=       |  百度搜索
```

### 配置说明

#### 识别码条目

每个条目由 `识别码 = 值` 组成。输入识别码（前缀匹配），按回车执行。

值支持以下类型，自动识别：

| 值特征 | 行为 |
|---|---|
| 以 `http://` 或 `https://` 开头（不含 `?`） | 浏览器打开该网址 |
| 以 `http://` 或 `https://` 开头（含 `?`） | 浏览器打开，可输入搜索词 |
| 以 `.exe` 结尾 | 启动该程序 |
| 已存在的文件夹路径 | 打开文件夹 |
| 已存在的文件路径 | 默认程序打开 |
| 以上都不是 | 报错，不执行 |

#### 分类分组

使用 `[分类名]` 区段头对条目分组，方便管理：

```toml
[网址]
gh = https://github.com

[程序]
notepad = C:\Windows\System32\notepad.exe
```

区段名会显示在列表中作为分类标签，不影响搜索匹配。

#### 描述语法

value 后可用 ` | `（空格-竖线-空格）添加描述，列表中将显示描述而非原始 value：

```toml
gh = https://github.com  |  GitHub首页
calc = C:\Windows\System32\calc.exe  |  计算器
```

#### 搜索引擎

带搜索参数的 URL（含 `?`）可以在识别码后加空格输入关键词：

```
gg Rust入门教程
```

程序会拼接出 `https://www.google.com/search?q=Rust入门教程` 并用浏览器打开。

#### 热键格式

```
修饰键 + 修饰键 + ... + 按键
```

- 修饰键：`Alt` `Ctrl` `Shift` `Win`
- 按键：`A~Z` `0~9` `F1~F24` `Space` `Enter` `Esc` `Tab` `Backspace` `Delete` `Insert` `Home` `End` `PageUp` `PageDown` `方向键` `符号键`
- 示例：`Alt+Space` `Ctrl+J` `Ctrl+Shift+F` `Alt+F1`

#### 黑名单

指定程序在前台时热键不响应，防止游戏误触：

```toml
_blacklist = notepad.exe,calc.exe
```

#### 私有字体

`fonts/` 目录下放 `.ttf` 或 `.otf` 文件，程序自动注册为私有字体（免安装，仅当前进程可见）。

不配 `_font` 时按文件名排序取第一个字体自动生效。多个字体时可显式指定：

```toml
_font = 得意黑
```

> 字体家族名称怎么看？双击字体文件，窗口顶部显示的名称就是。

---

## 系统托盘

- 左键点击托盘图标：呼出/隐藏
- 右键点击托盘图标：菜单（打开 Gua / 打开配置文件 / 退出）
- 托盘图标默认用系统图标，同目录放 `gua.ico` 即可替换

---

## 从源码构建

```bash
cargo build --release
```

编译产物为 `target/release/gua.exe`，仅 Windows 平台。

---

## 开源协议

MIT
