use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Serialize, Deserialize)]
pub struct MysqlConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MysqlResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub affected_rows: u64,
}

#[tauri::command]
pub async fn mysql_test_connection(config: MysqlConfig) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("无法启动 MySQL 测试任务：{e}"))?;

        runtime.block_on(mysql_test_connection_isolated(config))
    })
    .await
    .map_err(|e| format!("MySQL 测试任务异常结束：{e}"))?
}

async fn mysql_test_connection_isolated(config: MysqlConfig) -> Result<bool, String> {
    let host = config.host.trim();
    if host.is_empty() {
        return Err("MySQL 主机不能为空".to_string());
    }

    let mut opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(host)
        .tcp_port(config.port)
        .user(Some(&config.user))
        .pass(Some(&config.password));

    if !config.database.trim().is_empty() {
        opts = opts.db_name(Some(&config.database));
    }

    tokio::time::timeout(std::time::Duration::from_secs(8), async move {
        use mysql_async::prelude::*;

        let mut conn = mysql_async::Conn::new(opts)
            .await
            .map_err(|e| format!("MySQL 连接失败：{e}"))?;
        conn.query_drop("SELECT 1")
            .await
            .map_err(|e| format!("MySQL 连接验证失败：{e}"))?;
        conn.disconnect()
            .await
            .map_err(|e| format!("关闭 MySQL 测试连接失败：{e}"))?;
        Ok(true)
    })
    .await
    .map_err(|_| "MySQL 连接超时（8 秒），请检查主机、端口、防火墙和 MySQL 监听地址".to_string())?
}

#[tauri::command]
pub async fn mysql_query(config: MysqlConfig, sql: String) -> Result<MysqlResult, String> {
    let opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(&config.host)
        .tcp_port(config.port)
        .user(Some(&config.user))
        .pass(Some(&config.password))
        .db_name(Some(&config.database));

    let pool = mysql_async::Pool::new(opts);
    let mut conn = pool.get_conn().await.map_err(|e| e.to_string())?;

    use mysql_async::prelude::*;
    let mut result = conn.query_iter(&sql).await.map_err(|e| e.to_string())?;
    let columns: Vec<String> = result
        .columns_ref()
        .iter()
        .map(|c| c.name_str().to_string())
        .collect();

    let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
    let result_rows: Vec<mysql_async::Row> = result.collect().await.map_err(|e| e.to_string())?;
    let affected_rows = result_rows.len() as u64;

    for row in &result_rows {
        let mut values = Vec::new();
        for i in 0..columns.len() {
            let val = mysql_row_to_json(row, i);
            values.push(val);
        }
        rows.push(values);
    }

    conn.disconnect().await.map_err(|e| e.to_string())?;
    pool.disconnect().await.map_err(|e| e.to_string())?;

    Ok(MysqlResult {
        columns,
        rows,
        affected_rows,
    })
}

fn mysql_row_to_json(row: &mysql_async::Row, index: usize) -> serde_json::Value {
    use mysql_async::Value;
    match row.as_ref(index) {
        Some(Value::NULL) => serde_json::Value::Null,
        Some(Value::Int(i)) => serde_json::Value::Number((*i).into()),
        Some(Value::UInt(u)) => serde_json::Value::Number((*u).into()),
        Some(Value::Float(f)) => serde_json::Number::from_f64(*f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Value::Double(d)) => serde_json::Number::from_f64(*d)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).to_string().into(),
        Some(Value::Date(y, m, d, h, mi, s, _)) => {
            format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, mi, s).into()
        }
        Some(Value::Time(neg, d, h, m, s, _)) => {
            let sign = if *neg { "-" } else { "" };
            format!("{}{}:{:02}:{:02}", sign, d * 24 + *h as u32, m, s).into()
        }
        None => serde_json::Value::Null,
    }
}

#[tauri::command]
pub async fn mysql_execute(config: MysqlConfig, sql: String) -> Result<u64, String> {
    let opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(&config.host)
        .tcp_port(config.port)
        .user(Some(&config.user))
        .pass(Some(&config.password))
        .db_name(Some(&config.database));

    let pool = mysql_async::Pool::new(opts);
    let mut conn = pool.get_conn().await.map_err(|e| e.to_string())?;

    use mysql_async::prelude::*;
    conn.exec_drop(&sql, ()).await.map_err(|e| e.to_string())?;
    let affected = conn.affected_rows();

    conn.disconnect().await.map_err(|e| e.to_string())?;
    pool.disconnect().await.map_err(|e| e.to_string())?;

    Ok(affected)
}

#[tauri::command]
pub async fn mysql_list_databases(config: MysqlConfig) -> Result<Vec<String>, String> {
    tokio::time::timeout(std::time::Duration::from_secs(10), async move {
        let opts = mysql_async::OptsBuilder::default()
            .ip_or_hostname(&config.host)
            .tcp_port(config.port)
            .user(Some(&config.user))
            .pass(Some(&config.password));

        let mut conn = mysql_async::Conn::new(opts)
            .await
            .map_err(|e| format!("MySQL 连接失败：{e}"))?;

        use mysql_async::prelude::*;
        let rows: Vec<String> = conn
            .query("SHOW DATABASES")
            .await
            .map_err(|e| format!("读取数据库列表失败：{e}"))?;

        let system_dbs = ["information_schema", "mysql", "performance_schema", "sys"];
        let databases: Vec<String> = rows
            .into_iter()
            .filter(|db| !system_dbs.contains(&db.as_str()))
            .collect();

        conn.disconnect().await.map_err(|e| e.to_string())?;
        Ok(databases)
    })
    .await
    .map_err(|_| "读取数据库列表超时（10 秒）".to_string())?
}

#[tauri::command]
pub async fn mysql_export(
    config: MysqlConfig,
    export_dir: String,
    table: Option<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || mysql_export_blocking(config, export_dir, table))
        .await
        .map_err(|e| format!("mysqldump task failed: {e}"))?
}

fn mysql_export_blocking(
    config: MysqlConfig,
    export_dir: String,
    table: Option<String>,
) -> Result<String, String> {
    std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let database_filename = sanitize_filename_component(&config.database);
    let filename = format!("{database_filename}_{timestamp}.sql");
    let file_path = Path::new(&export_dir).join(filename);
    let mysqldump = find_mysqldump().ok_or_else(|| {
        "找不到 mysqldump。请将它作为 App sidecar 放在主程序同目录，\
或设置 NALU_MYSQLDUMP_PATH，或安装 MySQL/MariaDB 客户端。"
            .to_string()
    })?;

    let defaults_file = create_mysql_defaults_file(&config)?;
    let output_file = match std::fs::File::create(&file_path) {
        Ok(file) => file,
        Err(error) => {
            let _ = std::fs::remove_file(&defaults_file);
            return Err(error.to_string());
        }
    };

    let mut command = Command::new(&mysqldump);
    command
        // MySQL requires this option to appear before all other options.
        .arg(format!(
            "--defaults-extra-file={}",
            defaults_file.to_string_lossy()
        ))
        .args([
            "--single-transaction",
            "--quick",
            "--routines",
            "--events",
            "--triggers",
            "--hex-blob",
        ])
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::piped());

    if mysqldump
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("mysqldump"))
    {
        command.arg("--set-gtid-purged=OFF");
    }

    command.arg(&config.database);

    if let Some(table) = table {
        command.arg(table);
    }

    let result = command.spawn().and_then(|child| child.wait_with_output());
    let _ = std::fs::remove_file(&defaults_file);

    let output = match result {
        Ok(output) => output,
        Err(error) => {
            let _ = std::fs::remove_file(&file_path);
            return Err(format!(
                "无法执行 mysqldump（{}）：{error}",
                mysqldump.display()
            ));
        }
    };

    if !output.status.success() {
        let _ = std::fs::remove_file(&file_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mysqldump 导出失败：{}", stderr.trim()));
    }

    Ok(file_path.to_string_lossy().to_string())
}

fn find_mysqldump() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("NALU_MYSQLDUMP_PATH").map(PathBuf::from)
        && is_executable_file(&path)
    {
        return Some(path);
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        for name in mysqldump_binary_names() {
            let path = directory.join(name);
            if is_executable_file(&path) {
                return Some(path);
            }
        }
    }

    for candidate in platform_mysqldump_candidates() {
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    find_on_path()
}

fn mysqldump_binary_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["mysqldump.exe", "mariadb-dump.exe"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["mysqldump", "mariadb-dump"]
    }
}

fn platform_mysqldump_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            "/opt/homebrew/opt/mysql-client/bin/mysqldump".into(),
            "/opt/homebrew/opt/mysql@8.4/bin/mysqldump".into(),
            "/opt/homebrew/opt/mysql@8.0/bin/mysqldump".into(),
            "/opt/homebrew/bin/mysqldump".into(),
            "/usr/local/opt/mysql-client/bin/mysqldump".into(),
            "/usr/local/opt/mysql@8.4/bin/mysqldump".into(),
            "/usr/local/opt/mysql@8.0/bin/mysqldump".into(),
            "/usr/local/bin/mysqldump".into(),
            "/opt/homebrew/bin/mariadb-dump".into(),
            "/usr/local/bin/mariadb-dump".into(),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        for base in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
        ]
        .into_iter()
        .flatten()
        {
            let base = PathBuf::from(base);
            for version in ["MySQL Server 9.0", "MySQL Server 8.4", "MySQL Server 8.0"] {
                candidates.push(
                    base.join("MySQL")
                        .join(version)
                        .join("bin")
                        .join("mysqldump.exe"),
                );
            }
        }
        return candidates;
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            "/usr/bin/mysqldump".into(),
            "/usr/local/bin/mysqldump".into(),
            "/usr/bin/mariadb-dump".into(),
            "/usr/local/bin/mariadb-dump".into(),
        ]
    }
}

fn find_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for name in mysqldump_binary_names() {
            let candidate = directory.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn create_mysql_defaults_file(config: &MysqlConfig) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("nalu-mysqldump-{}.cnf", uuid::Uuid::new_v4()));
    let contents = format!(
        "[client]\nhost=\"{}\"\nport={}\nuser=\"{}\"\npassword=\"{}\"\n",
        escape_mysql_option_value(&config.host),
        config.port,
        escape_mysql_option_value(&config.user),
        escape_mysql_option_value(&config.password),
    );

    std::fs::write(&path, contents).map_err(|e| format!("无法创建临时 MySQL 配置：{e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        {
            let _ = std::fs::remove_file(&path);
            return Err(format!("无法保护临时 MySQL 配置：{error}"));
        }
    }

    Ok(path)
}

fn escape_mysql_option_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn sanitize_filename_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        "database".to_string()
    } else {
        sanitized
    }
}

#[tauri::command]
pub async fn mysql_import(config: MysqlConfig, file_path: String) -> Result<u64, String> {
    let sql_content = std::fs::read_to_string(&file_path).map_err(|e| e.to_string())?;

    let opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname(&config.host)
        .tcp_port(config.port)
        .user(Some(&config.user))
        .pass(Some(&config.password))
        .db_name(Some(&config.database));

    let pool = mysql_async::Pool::new(opts);
    let mut conn = pool.get_conn().await.map_err(|e| e.to_string())?;

    use mysql_async::prelude::*;
    conn.query_drop(&sql_content)
        .await
        .map_err(|e| e.to_string())?;

    let statement_count = sql_content.matches(';').count() as u64;

    conn.disconnect().await.map_err(|e| e.to_string())?;
    pool.disconnect().await.map_err(|e| e.to_string())?;

    Ok(statement_count)
}
