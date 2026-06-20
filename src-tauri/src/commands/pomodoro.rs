use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomodoroState {
    pub is_running: bool,
    pub is_break: bool,
    pub remaining_seconds: u32,
    pub work_duration: u32,  // default 1500 (25 min)
    pub break_duration: u32, // default 300 (5 min)
    pub completed_count: u32,
}

// Use a global static for the timer state since we need it across commands
static POMODORO: std::sync::LazyLock<Mutex<PomodoroState>> = std::sync::LazyLock::new(|| {
    Mutex::new(PomodoroState {
        is_running: false,
        is_break: false,
        remaining_seconds: 1500,
        work_duration: 1500,
        break_duration: 300,
        completed_count: 0,
    })
});

// Track if a background task is spawned
static TIMER_TASK: std::sync::LazyLock<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

const POMODORO_STATE_CHANGED_EVENT: &str = "pomodoro-state-changed";

fn emit_state(app: &AppHandle, state: &PomodoroState) {
    let _ = app.emit_to("main", POMODORO_STATE_CHANGED_EVENT, state.clone());
}

#[tauri::command]
pub fn pomodoro_get_state() -> Result<PomodoroState, String> {
    let state = POMODORO.lock().map_err(|e| e.to_string())?;
    Ok(state.clone())
}

#[tauri::command]
pub fn pomodoro_start(app: AppHandle) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    if state.is_running {
        return Ok(state.clone());
    }
    if state.remaining_seconds == 0 {
        state.remaining_seconds = if state.is_break {
            state.break_duration
        } else {
            state.work_duration
        };
    }
    state.is_running = true;
    let started_state = state.clone();
    emit_state(&app, &started_state);
    drop(state);

    // Cancel any existing timer task
    {
        let mut task = TIMER_TASK.lock().map_err(|e| e.to_string())?;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }

    // Spawn background timer
    let app_clone = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let mut state = POMODORO.lock().unwrap();
            if !state.is_running {
                break;
            }
            state.remaining_seconds = state.remaining_seconds.saturating_sub(1);
            // Emit tick event (main window only to avoid duplicate handling)
            let _ = app_clone.emit_to("main", "pomodoro-tick", state.remaining_seconds);

            if state.remaining_seconds == 0 {
                // Timer finished
                state.is_running = false;
                if state.is_break {
                    // Break finished, wait for user confirmation before the next work session
                    state.is_break = false;
                    state.remaining_seconds = state.work_duration;
                    emit_state(&app_clone, &state);
                    let _ = app_clone.emit_to("main", "pomodoro-break-end", state.completed_count);
                } else {
                    // Work finished, wait for user confirmation before the break
                    state.completed_count += 1;
                    state.is_break = true;
                    state.remaining_seconds = state.break_duration;
                    emit_state(&app_clone, &state);
                    let _ = app_clone.emit_to("main", "pomodoro-work-end", state.completed_count);
                }
                break;
            }
        }
    });

    let mut task = TIMER_TASK.lock().map_err(|e| e.to_string())?;
    *task = Some(handle);
    Ok(started_state)
}

#[tauri::command]
pub fn pomodoro_pause(app: AppHandle) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    state.is_running = false;
    let state = state.clone();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub fn pomodoro_reset(app: AppHandle) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    state.is_running = false;
    state.is_break = false;
    state.remaining_seconds = state.work_duration;
    let state = state.clone();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub fn pomodoro_skip(app: AppHandle) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    if state.is_break {
        state.is_break = false;
        state.remaining_seconds = state.work_duration;
    } else {
        state.completed_count += 1;
        state.is_break = true;
        state.remaining_seconds = state.break_duration;
    }
    let state = state.clone();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub fn pomodoro_set_duration(
    app: AppHandle,
    work_minutes: u32,
    break_minutes: u32,
) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    state.work_duration = work_minutes * 60;
    state.break_duration = break_minutes * 60;
    if !state.is_running {
        state.remaining_seconds = state.work_duration;
    }
    let state = state.clone();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub fn pomodoro_reset_rounds(app: AppHandle) -> Result<PomodoroState, String> {
    let mut state = POMODORO.lock().map_err(|e| e.to_string())?;
    state.completed_count = 0;
    let state = state.clone();
    emit_state(&app, &state);
    Ok(state)
}
