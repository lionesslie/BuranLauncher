use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG_FILE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

pub fn init(log_file: PathBuf) {
    if let Some(parent) = log_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = OpenOptions::new().create(true).append(true).open(&log_file);
    let _ = LOG_FILE.set(Mutex::new(Some(log_file)));
    install_panic_hook();
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error(&format!("Yakalanmamış hata: {info}"));
        default_hook(info);
    }));
}

pub fn write(level: &str, message: &str) {
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{stamp}] [{level}] {message}");
    if let Some(lock) = LOG_FILE.get() {
        if let Ok(guard) = lock.lock() {
            if let Some(path) = guard.as_ref() {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
    }
    println!("{line}");
}

pub fn info(message: &str) {
    write("info", message);
}

pub fn warn(message: &str) {
    write("warn", message);
}

pub fn error(message: &str) {
    write("error", message);
}