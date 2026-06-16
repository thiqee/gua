# Gua

Windows平台上的效率小工具，呼出搜索框，输入自己设置好的简短识别码回车打开程序，文件，网站。也可以输入搜索引擎的识别码+空格+问题，快速搜索。AI做的。

---

## 功能

- **热键呼出**：默认 `Alt+Space`，可自定义任意修饰键组合
- **快速启动**：输入简短的识别码回车直接打开文件，网站
- **命令执行**：识别码的值用反引号 `` ` `` 包裹，回车即以命令行方式执行（如打开此电脑、运行 ipconfig）
- **拼音搜索**：输入拼音即可匹配中文条目，如 `bili` 匹配「哔哩哔哩」、`tengxun` 匹配「腾讯文档」
- **快捷搜索**：`识别码 关键词` 自动拼接识别码对应的搜索引擎 URL和搜索词。注意识别码后有空格
- **黑名单**：指定程序在前台时热键不响应，防止游戏误触
- **自定义字体**：字体文件放 `fonts/` 目录即可使用，免安装。多个字体文件需要在配置文件里指定一个
- **系统托盘**：后台运行，右键菜单退出

---

## 快速开始

1. 运行 `gua.exe`，程序自动隐藏到系统托盘
2. 按热键（默认 `Alt+Space`）呼出搜索框
3. 输入识别码（如 `gh`），按 `Enter` 执行

首次运行需要先创建 `config.toml`（见下方配置说明），否则没有条目可启动。

---

## 安装

从 [Releases](https://github.com/thiqee/gua/releases) 下载最新版本exe，双击运行，无需安装。

推荐目录结构：

```
gua/
├── gua.exe
├── config.toml      # 配置文件（需自行创建）
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
# 热键，格式：修饰键+按键
_hotkey = Alt+Space
# 黑名单，逗号分隔 exe 名
_blacklist =
# 字体家族名称，双击字体文件查看左上角
_font = Segoe UI
# 字号
_font_size = 18

# 窗口宽度（像素）
_width = 500
# 窗口圆角大小
_round_corner = 12
# 窗口置顶
_always_on_top = true
# 透明度 0~255
_opacity = 255
# 列表最多显示条数
_max_results = 8
# 匹配是否区分大小写
_case_sensitive = true
# 失去焦点自动隐藏
_hide_on_focus_loss = true
# 状态栏文字字号
_status_font_size = 12

# 窗口背景色（RGB 十六进制）
_theme_color = 1E1E1E
# 输入框背景色
_input_bg_color = 2A2A2A
# 选中项高亮色
_accent_color = 4A6FA5
# 文字颜色
_text_color = CCCCCC

# 面板水平位置 0~100
_panel_position_x = 50
# 面板垂直位置 0~100
_panel_position_y = 50

# ─── 网址 ───
[网址]
gh = https://github.com
bi = https://www.bilibili.com

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
| 以 `` ` `` 开头和结尾 | 作为命令行执行（去掉反引号后传给 `CreateProcessW`） |
| 以 `http://` 或 `https://` 开头（不含 `?`） | 浏览器打开该网址 |
| 以 `http://` 或 `https://` 开头（含 `?`） | 浏览器打开，可输入搜索词 |
| 以 `.exe` 结尾 | 启动该程序 |
| 已存在的文件夹路径 | 打开文件夹 |
| 已存在的文件路径 | 默认程序打开 |
| 以上都不是 | 不执行 |

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

#### 搜索匹配

输入识别码时，Gua 按以下层级匹配条目，精确结果永远排在最前：

| 层级 | 匹配方式 | 示例（输入 `bili`） |
|---|---|---|
| 1 | 精确匹配 | `bili` 精确命中 |
| 2 | 前缀匹配 | `bilibili` 开头匹配 |
| 3 | 子串匹配 | `abcli` 包含匹配 |
| 4 | 拼音前缀 | `哔哩哔哩`（拼音 `bilibili` 前缀匹配） |
| 5 | 拼音子串 | `哔哩`（拼音 `bili` 包含 `bi`） |
| 6 | 模糊匹配 | `BiliHub` 字符按顺序出现 |

拼音匹配自动将中文条目名转为拼音，无需手动标注。支持中英文混排条目如 `B站` → `Bzhan`。

#### 命令执行

值用反引号 `` ` `` 包裹时，Gua 将其作为命令行执行，支持程序名+参数。适用于打开此电脑、控制面板等系统特殊位置，或运行控制台命令。

```toml
[系统]
此电脑 = `explorer shell:MyComputerFolder`
ipconfig = `ipconfig`
树形结构 = `tree E:\projects /f`
```

- 命令不需要 `.exe` 后缀（系统会自动从 PATH 中查找）
- 带空格的程序路径用引号包裹，如 `` `"C:\Program Files\App\app.exe" --flag` ``
- 控制台命令会弹出终端窗口显示输出。需要窗口执行完不关，配合 `cmd /k` 使用：

```toml
ipconfig_keep = `cmd /k ipconfig`
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

---

## 从源码构建

```bash
cargo build --release
```

编译产物为 `target/release/gua.exe`，仅 Windows 平台。

---

## 开源协议

MIT
