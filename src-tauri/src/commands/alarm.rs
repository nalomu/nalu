use crate::db::database::get_connection;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Alarm {
    pub id: String,
    pub time: String,
    pub label: String,
    pub repeat: String,
    pub active: bool,
    pub skip_next: bool,
    pub sound: Option<String>,
    pub created_at: String,
}

/// Tracks (alarm_id, minute_string) pairs to prevent duplicate triggers within the same minute.
static FIRED_ALARMS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

#[tauri::command]
pub fn get_alarms() -> Result<Vec<Alarm>, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, time, label, repeat, active, COALESCE(skip_next,0), sound, created_at FROM alarms ORDER BY time ASC")
        .map_err(|e| e.to_string())?;
    let alarms = stmt
        .query_map([], |row| {
            Ok(Alarm {
                id: row.get(0)?,
                time: row.get(1)?,
                label: row.get(2)?,
                repeat: row.get(3)?,
                active: row.get::<_, i32>(4)? != 0,
                skip_next: row.get::<_, i32>(5)? != 0,
                sound: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(alarms)
}

#[tauri::command]
pub fn add_alarm(
    time: String,
    label: String,
    repeat: String,
    sound: Option<String>,
) -> Result<Alarm, String> {
    let id = uuid::Uuid::new_v4().to_string();

    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "INSERT INTO alarms (id, time, label, repeat, sound) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, time, label, repeat, sound],
    )
    .map_err(|e| e.to_string())?;

    Ok(Alarm {
        id,
        time,
        label,
        repeat,
        active: true,
        skip_next: false,
        sound,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
pub fn toggle_alarm(id: String) -> Result<bool, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE alarms SET active = 1 - active WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    let active: bool = conn
        .query_row(
            "SELECT active FROM alarms WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, i32>(0),
        )
        .map_err(|e| e.to_string())?
        != 0;
    Ok(active)
}

fn deactivate_alarm(id: &str) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE alarms SET active = 0 WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn skip_next_alarm(id: String) -> Result<Alarm, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    // Toggle: re-clicking un-sets the skip
    conn.execute(
        "UPDATE alarms SET skip_next = 1 - COALESCE(skip_next, 0) WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, time, label, repeat, active, COALESCE(skip_next,0), sound, created_at FROM alarms WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Alarm {
                id: row.get(0)?,
                time: row.get(1)?,
                label: row.get(2)?,
                repeat: row.get(3)?,
                active: row.get::<_, i32>(4)? != 0,
                skip_next: row.get::<_, i32>(5)? != 0,
                sound: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_alarm_sound(id: String, sound: Option<String>) -> Result<Alarm, String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute(
        "UPDATE alarms SET sound = ?1 WHERE id = ?2",
        rusqlite::params![sound, id],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT id, time, label, repeat, active, COALESCE(skip_next,0), sound, created_at FROM alarms WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Alarm {
                id: row.get(0)?,
                time: row.get(1)?,
                label: row.get(2)?,
                repeat: row.get(3)?,
                active: row.get::<_, i32>(4)? != 0,
                skip_next: row.get::<_, i32>(5)? != 0,
                sound: row.get(6)?,
                created_at: row.get(7)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_alarm(id: String) -> Result<(), String> {
    let db = get_connection()?;
    let conn = db.as_ref().unwrap();
    conn.execute("DELETE FROM alarms WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Background alarm checker ─────────────────────────────

fn is_day_match(repeat: &str) -> bool {
    let day = chrono::Local::now().weekday().num_days_from_sunday(); // 0=Sun
    match repeat {
        "none" | "daily" => true,
        "weekdays" => (1..=5).contains(&day),
        "weekends" => day == 0 || day == 6,
        _ => false,
    }
}

/// Starts a background task that checks alarms every 10 seconds.
/// Emits `alarm-triggered` event to the frontend when an alarm matches.
/// Runs on the Rust side so it's immune to WebView JS throttling.
pub fn start_alarm_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            let now = chrono::Local::now();
            let current_hhmm = now.format("%H:%M").to_string();
            let minute_key = now.format("%H:%M:%Y%m%d").to_string();

            if let Ok(alarms) = get_alarms() {
                for alarm in &alarms {
                    if !alarm.active || alarm.time != current_hhmm || !is_day_match(&alarm.repeat) {
                        continue;
                    }

                    // If this alarm is marked skip-next, reset the flag and skip triggering.
                    // The alarm stays active — just this one cycle is skipped.
                    if alarm.skip_next {
                        if let Ok(db) = get_connection()
                            && let Some(conn) = db.as_ref()
                        {
                            let _ = conn.execute(
                                "UPDATE alarms SET skip_next = 0 WHERE id = ?1",
                                rusqlite::params![alarm.id],
                            );
                        }
                        tracing::info!("[AlarmChecker] skipped: {} ({})", alarm.label, alarm.id);
                        continue;
                    }

                    // Dedup: check if we already fired this alarm in this minute
                    let already_fired = if let Ok(fired) = FIRED_ALARMS.lock() {
                        fired
                            .iter()
                            .any(|(id, min)| id == &alarm.id && min == &minute_key)
                    } else {
                        false
                    };

                    if !already_fired {
                        if let Ok(mut fired) = FIRED_ALARMS.lock() {
                            fired.push((alarm.id.clone(), minute_key.clone()));
                            // Prune old entries
                            if fired.len() > 100 {
                                *fired = fired[fired.len().saturating_sub(50)..].to_vec();
                            }
                        }
                        // Only emit to the main window to prevent duplicate audio
                        // loops across multiple webviews (e.g. clipboard-popup)
                        let _ = app.emit_to("main", "alarm-triggered", alarm);
                        tracing::info!("[AlarmChecker] triggered: {} ({})", alarm.label, alarm.id);

                        // Non-repeating alarms are one-shot — disable them after firing
                        // so they don't ring again the next day.
                        if alarm.repeat == "none" {
                            if let Err(e) = deactivate_alarm(&alarm.id) {
                                tracing::warn!(
                                    "[AlarmChecker] deactivate non-repeating alarm failed: {}",
                                    e
                                );
                            } else {
                                tracing::info!(
                                    "[AlarmChecker] non-repeating alarm disabled: {}",
                                    alarm.id
                                );
                                // Tell any open page that listens for data changes (alarm list, dashboard) to reload.
                                let _ = app.emit("ai-data-changed", ());
                            }
                        }
                    }
                }
            }
        }
    });
}

// ── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_day_match_none() {
        assert!(is_day_match("none"));
    }

    #[test]
    fn test_is_day_match_daily() {
        assert!(is_day_match("daily"));
    }

    #[test]
    fn test_is_day_match_weekdays_vs_weekends_exclusive() {
        let wd = is_day_match("weekdays");
        let we = is_day_match("weekends");
        assert!(!(wd && we), "weekdays and weekends cannot both be true");
        assert!(wd || we, "exactly one of weekdays/weekends must be true");
    }

    #[test]
    fn test_is_day_match_unknown() {
        assert!(!is_day_match("monthly"));
        assert!(!is_day_match(""));
    }

    #[test]
    fn test_fired_alarms_dedup() {
        let mut fired: Vec<(String, String)> = Vec::new();
        let alarm_id = "test-alarm-1".to_string();
        let minute_key = "10:30:20260607".to_string();

        let already = fired
            .iter()
            .any(|(id, min)| id == &alarm_id && min == &minute_key);
        assert!(!already);

        fired.push((alarm_id.clone(), minute_key.clone()));

        let already = fired
            .iter()
            .any(|(id, min)| id == &alarm_id && min == &minute_key);
        assert!(already);

        let different_minute = "10:31:20260607".to_string();
        let already = fired
            .iter()
            .any(|(id, min)| id == &alarm_id && min == &different_minute);
        assert!(!already);
    }

    #[test]
    fn test_fired_alarms_pruning() {
        let mut fired: Vec<(String, String)> = Vec::new();
        for i in 0..120 {
            fired.push((format!("alarm-{}", i), format!("10:{:02}:20260607", i % 60)));
        }
        if fired.len() > 100 {
            fired = fired[fired.len().saturating_sub(50)..].to_vec();
        }
        assert_eq!(fired.len(), 50);
        assert_eq!(fired[0].0, "alarm-70");
    }

    #[test]
    fn test_alarm_time_matching() {
        let alarm = Alarm {
            id: "a1".into(),
            time: "14:30".into(),
            label: "test".into(),
            repeat: "daily".into(),
            active: true,
            skip_next: false,
            sound: None,
            created_at: "".into(),
        };
        assert!(alarm.active && alarm.time == "14:30" && is_day_match(&alarm.repeat));
        assert!(alarm.time != "14:31");

        let inactive = Alarm {
            active: false,
            ..alarm.clone()
        };
        assert!(!inactive.active);
    }
}
