// Gua Plugin API — C 头文件
// 插件编译为 DLL，导出 gua_plugin_load 函数
//
// 使用示例（C）：
//   #include "gua-api.h"
//   static const GuaApi* api;
//   static int32_t my_hotkey_id;
//
//   int32_t GUA_PLUGIN_EXPORT gua_plugin_load(const GuaApi* a, PluginVtable* v) {
//       api = a;
//       v->vtable_size = sizeof(PluginVtable);
//   v->init = my_init;
//       v->on_hotkey = my_on_hotkey;
//       return 0;
//   }
//
//   int32_t my_init(void) {
//       my_hotkey_id = api->register_hotkey(1, 0x4A, 0);  // MOD_ALT + VK_J
//       return 0;
//   }

#ifndef GUA_API_H
#define GUA_API_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ── 版本常量 ────────────────────────────────────────────────

#define GUA_API_VERSION 1

// ── 热键修饰键 ──────────────────────────────────────────────

#define GUA_MOD_ALT    1
#define GUA_MOD_CTRL   2
#define GUA_MOD_SHIFT  4
#define GUA_MOD_WIN    8

// ── 日志级别 ────────────────────────────────────────────────

#define GUA_LOG_INFO    0
#define GUA_LOG_WARN    1
#define GUA_LOG_ERROR   2

// ── API 表（Gua → 插件） ─────────────────────────────────────

typedef struct {
    uint32_t api_version;             // = GUA_API_VERSION
    uint32_t struct_size;             // sizeof(GuaApi)

    // 注册热键：mods 是 GUA_MOD_* 位掩码，vk 是虚拟键码
    // 返回 user_id（成功）或 -1（失败）
    int32_t (*register_hotkey)(uint32_t mods, uint32_t vk, int32_t user_id);

    // 注销热键
    void    (*unregister_hotkey)(int32_t user_id);

    // 读取插件配置项
    //   key: UTF-8 配置项名
    //   buf: 输出缓冲区（可为 NULL 只查询长度）
    //   buf_size: 缓冲区大小
    // 返回值：
    //   >= 0  key 存在，返回写入 buf 的字节数（不含 \0）
    //   -1    key 不存在
    //   -2    buf_size 不够
    // 编码: UTF-8，末尾带 \0
    int32_t (*get_config)(const char* key, char* buf, int32_t buf_size);

    // 注册定时器（毫秒级），返回 timer_id 或 -1
    int32_t (*set_timer)(uint32_t interval_ms, int32_t user_id);

    // 取消定时器
    void    (*kill_timer)(int32_t user_id);

    // 日志输出
    //   msg 指针只在调用期间有效，Gua 会立即复制
    void    (*log)(int32_t level, const char* msg);

    // Gua 主窗口的 HWND（插件可发消息或做 Win32 操作）
    uint64_t hwnd;
} GuaApi;

// ── 回调表（插件 → Gua） ─────────────────────────────────────

typedef struct {
    uint32_t vtable_size;             // 调用方写入 sizeof(PluginVtable)

    // 初始化，在此函数中注册热键
    // 返回 0 表示成功
    int32_t (*init)(void);

    // 热键被触发
    void    (*on_hotkey)(int32_t user_id);

    // 定时器触发
    void    (*on_tick)(int32_t user_id);

    // 配置热重载通知
    void    (*on_config_reload)(void);

    // 插件卸载前清理
    void    (*cleanup)(void);

    // 窗口消息钩子（可选），返回 true 表示已处理
    int32_t (*on_wndproc)(uint32_t msg, uint64_t wp, int64_t lp);
} PluginVtable;

// ── 插件导出入口 ──────────────────────────────────────────────

// 插件必须导出的唯一函数
// 返回值: 0=成功，非0=失败
typedef int32_t (GUA_PLUGIN_ENTRY*)(const GuaApi* api, PluginVtable* vtable);

#define GUA_PLUGIN_EXPORT __declspec(dllexport)

#ifdef __cplusplus
}
#endif

#endif // GUA_API_H
