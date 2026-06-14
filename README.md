# Gua

快捷键启动器。按热键弹出搜索框，输入识别码快速打开网址、程序、文件或文件夹。

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

## 下载

从 [Releases](https://github.com/你的用户名/gua/releases) 下载最新版本。

解压即可运行，无需安装。

```
gua/
├── gua.exe
├── config.toml      # 配置文件
├── gua.ico          # 托盘图标（可选，不配则用默认）
└── fonts/           # 字体目录（可选）
    ├── LXGWWenKai-Regular.ttf
    └── SmileySans-Oblique.ttf
```

---

## 快速开始

1. 运行 `gua.exe`，程序自动隐藏到系统托盘
2. 按热键（默认 `Ctrl+J`）呼出搜索框
3. 输入识别码（如 `gh`），按 `Enter` 执行

修改 `config.toml` 后按热键或点托盘图标即可热重载，无需重启。

---

## 配置

`config.toml` 支持识别码条目和内部配置键。

### 识别码条目

```toml
[网址]
gh = https://github.com
b23 = https://www.bilibili.com

[程序]
calc = C:\Windows\System32\calc.exe

[搜索引擎]
gg = https://www.google.com/search?q=
```

### 常用配置项

| 配置键 | 作用 | 默认值 |
|---|---|---|
| `_hotkey` | 热键组合 | `Alt+Space` |
| `_blacklist` | 黑名单程序（逗号分隔） | 空 |
| `_font` | 字体家族名称 | `Segoe UI` |
| `_theme_color` | 窗口背景色（RGB） | `1E1E1E` |
| `_opacity` | 窗口透明度 0~255 | `255` |

完整配置项见 `config.toml` 中的注释。

> 字体家族名称怎么看？双击 `.ttf` / `.otf` 文件，窗口顶部显示的名称就是。

---

## 从源码构建

```bash
cargo build --release
```

编译产物为 `target/release/gua.exe`，仅 Windows 平台。

---

## 开源协议

MIT
