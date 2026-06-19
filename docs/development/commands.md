# Tauri Commands

Tauri commands 是前端调用 Rust 后端的边界。命令注册集中在 `src-tauri/src/lib.rs`。

## 提醒相关

| 命令 | 说明 |
|------|------|
| `get_alarms` | 获取闹钟列表 |
| `add_alarm` | 新增闹钟，支持单独铃声 |
| `update_alarm_sound` | 更新单个闹钟铃声 |
| `toggle_alarm` | 启用或禁用闹钟 |
| `skip_next_alarm` | 跳过下一次重复闹钟 |
| `delete_alarm` | 删除闹钟 |
| `pomodoro_start` | 开始番茄钟 |
| `pomodoro_pause` | 暂停番茄钟 |
| `pomodoro_reset` | 重置番茄钟 |

## 声音文件

| 命令 | 说明 |
|------|------|
| `check_path_exists` | 检查本地路径是否存在 |
| `copy_custom_sound` | 复制自定义铃声到应用数据目录 |

## 平台限制

Android 不支持桌面端的全局快捷键、系统托盘和 NSPanel。相关命令在移动端返回空操作或不支持错误，前端应避免把这些能力作为移动端主流程。
