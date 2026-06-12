# KeyHop 使用手册

## 简介

KeyHop 是一个 Windows 快捷键启动器。按 `Alt+Space` 呼出搜索框，输入识别码就能快速打开网址、程序、文件或文件夹。支持搜索引擎，输入 `gg 关键词` 可直接用 Google 搜索。

---

## 基本操作

### 呼出 / 隐藏

| 操作 | 效果 |
|---|---|
| 按 `Alt+Space` | 呼出搜索框，再次按下或按 `Esc` 隐藏 |
| 点击输入框外部（失去焦点） | 自动隐藏 |
| 点击系统托盘图标 | 呼出 / 隐藏切换 |

### 搜索启动

1. 呼出搜索框，输入识别码（如 `gh`）
2. 下方列表实时显示匹配结果
3. 按 `↓` `↑` 选择条目，按 `Enter` 执行
4. 按 `Esc` 取消并隐藏

### 搜索引擎

配置文件中有搜索引擎条目（如 `gg = https://www.google.com/search?q=`），在搜索框里输入：

```
gg 今天天气
```

程序会拼接出 `https://www.google.com/search?q=今天天气` 并用浏览器打开。

### 光标操作搜索框支持基本的文本编辑：

| 按键 | 效果 |
|---|---|
| `←` `→` | 在输入文本中移动光标 |
| `Backspace` | 删除光标前一个字符 |
| `Delete` | 删除光标后一个字符 |
| 输入字符 | 在光标位置插入 |

### 系统托盘

右键点击托盘图标 → 菜单：

| 菜单项 | 效果 |
|---|---|
| 打开 KeyHop | 呼出搜索框 |
| 打开配置文件 | 用默认编辑器打开 `config.toml` |
| 退出 | 完全退出程序 |

---

## 配置文件

配置文件是 `config.toml`，和程序 exe 在同一目录。修改后按 `Alt+Space` 会自动重载，无需重启。

### 识别码条目

格式：

```toml
[分类名]
识别码 = 路径或网址
```

- `[分类名]` 仅用于分组，不参与搜索，可以随便写
- `#` 开头的行是注释
- 空行会被忽略
- `=` 左右允许有空格

示例：

```toml
[网址]
gh  = https://github.com
b23 = https://www.bilibili.com

[程序]
calc    = C:\Windows\System32\calc.exe
notepad = C:\Windows\System32\notepad.exe

[搜索引擎]
gg = https://www.google.com/search?q=
bd = https://www.baidu.com/s?wd=
```

值类型自动识别：

| 值特征 | 打开方式 |
|---|---|
| 以 `http://` 或 `https://` 开头 | 默认浏览器打开 |
| 以 `.exe` 结尾 | 启动该程序 |
| 已存在的文件夹路径 | 打开文件夹 |
| 已存在的文件路径 | 默认程序打开 |
| 以上都不是 | 报错，不执行 |

### 界面配置

以下配置项以 `_` 开头，放在配置文件的任意位置。

#### 字体

```toml
_font = Microsoft YaHei       # 字体名称，默认 Segoe UI
_font_size = 18                # 输入框字号，默认 18
_status_font_size = 12         # 状态栏字号（如"第3条/共10条"），默认 12
```

#### 窗口外观

```toml
_width = 500                   # 窗口宽度（像素），默认 500
_round_corner = 12             # 窗口圆角大小，默认 12
_theme_color = 1E1E1E          # 窗口背景色（RGB 十六进制），默认 1E1E1E
_input_bg_color = 2A2A2A       # 输入框背景色，默认 2A2A2A
_accent_color = 4A6FA5         # 选中项高亮色，默认 4A6FA5
_text_color = CCCCCC           # 文字颜色，默认 CCCCCC
```

颜色值用 6 位十六进制（RGB），如 `FF0000` 红色、`00FF00` 绿色、`0000FF` 蓝色。

#### 面板位置

```toml
_panel_position_x = 50         # 水平位置 0~100（0=左边缘 50=居中 100=右边缘），默认 50
_panel_position_y = 50         # 垂直位置 0~100（0=顶边缘 50=居中 100=底边缘），默认 50
```

#### 行为

```toml
_always_on_top = true          # 窗口是否置顶，默认 true
_opacity = 255                 # 窗口透明度 0~255（255=不透明），默认 255
_max_results = 8               # 结果列表最多显示条数，默认 8
_case_sensitive = false        # 匹配是否区分大小写，默认 true
_hide_on_focus_loss = true     # 失去焦点时是否自动隐藏，默认 true
```

---

## 配置文件热重载

每次按 `Alt+Space` 呼出时，程序会自动检查 `config.toml` 的修改时间。有变化才重新读取，没变化直接用内存里的配置。所以改完配置后，按一下 `Alt+Space` 就能生效。

---

## 常见问题

**Q: 程序怎么退出？**
右键托盘图标 → 退出。或者直接结束进程。

**Q: 配置改了没生效？**
确认格式正确（`=` 两边别漏了），保存文件后再按一次 `Alt+Space`。

**Q: 窗口不见了？**
检查系统托盘，图标可能在折叠区里。点击图标或按 `Alt+Space` 重新呼出。

**Q: 识别码区分大小写吗？**
默认区分（`case_sensitive = true`），设为 `false` 则不区分。
