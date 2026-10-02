use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MysqlTransferProgress {
    pub operation_id: String,
    pub kind: &'static str,
    pub stage: &'static str,
    pub processed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MysqlImportResult {
    pub processed_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MysqlExportResult {
    pub file_path: String,
    pub processed_bytes: u64,
}

const MYSQL_TRANSFER_PROGRESS_EVENT: &str = "mysql-transfer-progress";
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(100);

fn emit_transfer_progress(
    app: &AppHandle,
    operation_id: &str,
    kind: &'static str,
    stage: &'static str,
    processed_bytes: u64,
    total_bytes: Option<u64>,
) {
    let percent = total_bytes.map(|total| {
        processed_bytes
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100)
            .min(100) as u8
    });

    let _ = app.emit(
        MYSQL_TRANSFER_PROGRESS_EVENT,
        MysqlTransferProgress {
            operation_id: operation_id.to_string(),
            kind,
            stage,
            processed_bytes,
            total_bytes,
            percent,
        },
    );
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
    app: AppHandle,
    operation_id: String,
    config: MysqlConfig,
    export_dir: String,
    table: Option<String>,
) -> Result<MysqlExportResult, String> {
    tokio::task::spawn_blocking(move || {
        mysql_export_blocking(app, operation_id, config, export_dir, table)
    })
    .await
    .map_err(|e| format!("mysqldump task failed: {e}"))?
}

fn mysql_export_blocking(
    app: AppHandle,
    operation_id: String,
    config: MysqlConfig,
    export_dir: String,
    table: Option<String>,
) -> Result<MysqlExportResult, String> {
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
            "--extended-insert",
            "--disable-keys",
            "--routines",
            "--events",
            "--triggers",
            "--hex-blob",
        ])
        .stdout(Stdio::piped())
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

    emit_transfer_progress(&app, &operation_id, "export", "starting", 0, None);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_file(&defaults_file);
            let _ = std::fs::remove_file(&file_path);
            return Err(format!(
                "无法执行 mysqldump（{}）：{error}",
                mysqldump.display()
            ));
        }
    };

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取 mysqldump 导出数据".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 mysqldump 错误输出".to_string())?;
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = BufReader::new(stderr).read_to_end(&mut bytes);
        bytes
    });

    let mut writer = BufWriter::new(output_file);
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut processed_bytes = 0_u64;
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;
    let copy_result: Result<(), String> = (|| {
        loop {
            let read = stdout
                .read(&mut buffer)
                .map_err(|e| format!("读取 mysqldump 导出数据失败：{e}"))?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .map_err(|e| format!("写入导出文件失败：{e}"))?;
            processed_bytes += read as u64;
            if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                emit_transfer_progress(
                    &app,
                    &operation_id,
                    "export",
                    "transferring",
                    processed_bytes,
                    None,
                );
                last_emit = Instant::now();
            }
        }
        writer
            .flush()
            .map_err(|e| format!("刷新导出文件失败：{e}"))?;
        Ok(())
    })();
    drop(writer);

    if copy_result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    let stderr = stderr_reader.join().unwrap_or_default();
    let _ = std::fs::remove_file(&defaults_file);

    if let Err(error) = copy_result {
        let _ = std::fs::remove_file(&file_path);
        return Err(error);
    }

    let status = match status {
        Ok(status) => status,
        Err(error) => {
            let _ = std::fs::remove_file(&file_path);
            return Err(format!("等待 mysqldump 结束失败：{error}"));
        }
    };

    if !status.success() {
        let _ = std::fs::remove_file(&file_path);
        let stderr = String::from_utf8_lossy(&stderr);
        return Err(format!("mysqldump 导出失败：{}", stderr.trim()));
    }

    emit_transfer_progress(
        &app,
        &operation_id,
        "export",
        "completed",
        processed_bytes,
        Some(processed_bytes),
    );

    Ok(MysqlExportResult {
        file_path: file_path.to_string_lossy().to_string(),
        processed_bytes,
    })
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
pub async fn mysql_import(
    app: AppHandle,
    operation_id: String,
    config: MysqlConfig,
    file_path: String,
) -> Result<MysqlImportResult, String> {
    tokio::task::spawn_blocking(move || mysql_import_blocking(app, operation_id, config, file_path))
        .await
        .map_err(|e| format!("导入任务异常结束：{e}"))?
}

fn mysql_import_blocking(
    app: AppHandle,
    operation_id: String,
    config: MysqlConfig,
    file_path: String,
) -> Result<MysqlImportResult, String> {
    let file_path = Path::new(&file_path);
    if !file_path.is_file() {
        return Err(format!("文件不存在：{}", file_path.display()));
    }
    if !file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("sql"))
        .unwrap_or(false)
    {
        return Err("请选择 .sql 文件".to_string());
    }

    let mysql = find_mysql().ok_or_else(|| {
        "找不到 mysql 客户端。\
请确保 MySQL/MariaDB 客户端已安装（mysqldump 同目录下通常有 mysql 命令），\
或设置 NALU_MYSQL_PATH 环境变量。"
            .to_string()
    })?;

    let total_bytes = file_path
        .metadata()
        .map_err(|e| format!("无法读取 SQL 文件信息：{e}"))?
        .len();
    let source = std::fs::File::open(file_path).map_err(|e| e.to_string())?;
    let defaults_file = create_mysql_defaults_file(&config)?;

    let mut command = Command::new(&mysql);
    command
        .arg(format!(
            "--defaults-extra-file={}",
            defaults_file.to_string_lossy()
        ))
        .args([
            "--silent",
            "--binary-mode=1",
            "--max-allowed-packet=1073741824",
            "--net-buffer-length=1048576",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    // SQL text compresses well; protocol compression usually improves remote imports.
    // Keep loopback imports uncompressed to avoid unnecessary CPU overhead.
    if !is_loopback_mysql_host(&config.host) {
        command.arg("--compress");
    }
    command.arg(&config.database);

    emit_transfer_progress(
        &app,
        &operation_id,
        "import",
        "starting",
        0,
        Some(total_bytes),
    );

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_file(&defaults_file);
            return Err(format!("无法执行 mysql（{}）：{error}", mysql.display()));
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法写入 mysql 导入进程".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取 mysql 错误输出".to_string())?;
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = BufReader::new(stderr).read_to_end(&mut bytes);
        bytes
    });

    let mut source = BufReader::new(source);
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut processed_bytes = 0_u64;
    let mut last_emit = Instant::now() - PROGRESS_EMIT_INTERVAL;
    let copy_result: Result<(), String> = (|| {
        loop {
            let read = source
                .read(&mut buffer)
                .map_err(|e| format!("读取 SQL 文件失败：{e}"))?;
            if read == 0 {
                break;
            }
            stdin
                .write_all(&buffer[..read])
                .map_err(|e| format!("向 mysql 写入 SQL 数据失败：{e}"))?;
            processed_bytes += read as u64;
            if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL || processed_bytes == total_bytes {
                emit_transfer_progress(
                    &app,
                    &operation_id,
                    "import",
                    "transferring",
                    processed_bytes,
                    Some(total_bytes),
                );
                last_emit = Instant::now();
            }
        }
        stdin
            .flush()
            .map_err(|e| format!("刷新 mysql 导入数据失败：{e}"))?;
        Ok(())
    })();
    drop(stdin);

    if copy_result.is_ok() {
        emit_transfer_progress(
            &app,
            &operation_id,
            "import",
            "finalizing",
            processed_bytes,
            Some(total_bytes),
        );
    }

    if copy_result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    let stderr = stderr_reader.join().unwrap_or_default();

    let _ = std::fs::remove_file(&defaults_file);

    if let Err(error) = copy_result {
        let stderr = String::from_utf8_lossy(&stderr);
        let trimmed = stderr.trim();
        if !trimmed.is_empty() {
            return Err(format!("MySQL 导入错误：{trimmed}"));
        }
        return Err(error);
    }

    let status = status.map_err(|e| format!("等待 mysql 导入结束失败：{e}"))?;
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        let trimmed = stderr.trim();
        if trimmed.is_empty() {
            return Err("mysql 导入失败，无错误信息".to_string());
        }
        return Err(format!("MySQL 导入错误：{trimmed}"));
    }

    emit_transfer_progress(
        &app,
        &operation_id,
        "import",
        "completed",
        processed_bytes,
        Some(total_bytes),
    );

    Ok(MysqlImportResult { processed_bytes })
}

fn find_mysql() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("NALU_MYSQL_PATH").map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }

    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        for name in mysql_binary_names() {
            let path = directory.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    for candidate in platform_mysql_candidates() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    find_on_path_mysql()
}

fn mysql_binary_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["mysql.exe", "mariadb.exe"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["mysql", "mariadb"]
    }
}

fn platform_mysql_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            "/opt/homebrew/opt/mysql-client/bin/mysql".into(),
            "/opt/homebrew/opt/mysql@8.4/bin/mysql".into(),
            "/opt/homebrew/opt/mysql@8.0/bin/mysql".into(),
            "/opt/homebrew/bin/mysql".into(),
            "/usr/local/opt/mysql-client/bin/mysql".into(),
            "/usr/local/opt/mysql@8.4/bin/mysql".into(),
            "/usr/local/opt/mysql@8.0/bin/mysql".into(),
            "/usr/local/bin/mysql".into(),
            "/opt/homebrew/bin/mariadb".into(),
            "/usr/local/bin/mariadb".into(),
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
                        .join("mysql.exe"),
                );
            }
        }
        candidates
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            "/usr/bin/mysql".into(),
            "/usr/local/bin/mysql".into(),
            "/usr/bin/mariadb".into(),
            "/usr/local/bin/mariadb".into(),
        ]
    }
}

fn find_on_path_mysql() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for name in mysql_binary_names() {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_loopback_mysql_host(host: &str) -> bool {
    matches!(
        host.trim()
            .trim_matches(['[', ']'])
            .to_ascii_lowercase()
            .as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

#[cfg(test)]
mod tests {
    use super::is_loopback_mysql_host;

    #[test]
    fn detects_loopback_mysql_hosts() {
        for host in ["localhost", "LOCALHOST", "127.0.0.1", "::1", "[::1]"] {
            assert!(is_loopback_mysql_host(host), "{host} should be loopback");
        }
    }

    #[test]
    fn treats_lan_and_remote_mysql_hosts_as_remote() {
        for host in ["192.168.1.10", "mysql.internal", "10.0.0.8"] {
            assert!(!is_loopback_mysql_host(host), "{host} should be remote");
        }
    }
}
