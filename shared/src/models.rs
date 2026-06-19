use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub project: String,
    pub title: String,
    pub done: bool,
    pub progress: i32,
    pub column_id: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskColumn {
    pub id: String,
    pub project: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColumnWithTasks {
    pub column: TaskColumn,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroupData {
    pub project: String,
    pub sort_order: i64,
    pub columns: Vec<ColumnWithTasks>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskSnapshot {
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColumnSnapshot {
    pub column: TaskColumn,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub note_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Schedule {
    pub id: String,
    pub title: String,
    pub scheduled_at: String,
    pub reminder_minutes: i32,
    pub done: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Alarm {
    pub id: String,
    pub time: String,
    pub label: String,
    pub repeat: String,
    pub active: bool,
    pub skip_next: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PomodoroState {
    pub is_running: bool,
    pub is_break: bool,
    pub remaining_seconds: i64,
    pub work_duration: i64,
    pub break_duration: i64,
    pub completed_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MysqlUser {
    pub id: String,
    pub username: String,
    pub password: String,
    pub databases: String,
    pub created_at: String,
}
