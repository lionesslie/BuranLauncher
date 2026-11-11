use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Store {
    path: PathBuf,
    data: Mutex<Value>,
}

fn defaults() -> Value {
    json!({
        "language": null,
        "settings": {
            "ramMin": 1024,
            "ramMax": 4096,
            "javaPath": "",
            "extraArgs": "",
            "launchBehavior": "minimize",
            "startOnBoot": false,
            "minimizeToTray": true,
            "backgroundEnabled": true,
            "backgroundPath": null,
            "discordPresenceEnabled": true
        },
        "accounts": [],
        "activeAccountId": null,
        "instances": [],
        "servers": []
    })
}

fn merge_defaults(target: &mut Value, source: &Value) {
    if let (Value::Object(target_map), Value::Object(source_map)) = (target, source) {
        for (key, source_value) in source_map {
            match target_map.get_mut(key) {
                Some(target_value) => merge_defaults(target_value, source_value),
                None => {
                    target_map.insert(key.clone(), source_value.clone());
                }
            }
        }
    }
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let mut data = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        merge_defaults(&mut data, &defaults());
        let store = Store {
            path,
            data: Mutex::new(data),
        };
        store.save();
        store
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Value> {
        self.data.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn save_locked(&self, data: &Value) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(data) {
            let _ = fs::write(&self.path, raw);
        }
    }

    pub fn save(&self) {
        let data = self.lock();
        self.save_locked(&data);
    }

    pub fn get(&self, key: &str) -> Value {
        self.lock().get(key).cloned().unwrap_or(Value::Null)
    }

    pub fn set(&self, key: &str, value: Value) {
        {
            let mut data = self.lock();
            if !data.is_object() {
                *data = json!({});
            }
            data[key] = value;
            self.save_locked(&data);
        }
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get(key).as_str().map(ToString::to_string)
    }

    pub fn settings(&self) -> Value {
        let settings = self.get("settings");
        if settings.is_object() {
            settings
        } else {
            defaults()["settings"].clone()
        }
    }

    pub fn get_setting(&self, key: &str) -> Value {
        self.settings().get(key).cloned().unwrap_or(Value::Null)
    }

    pub fn get_setting_string(&self, key: &str) -> Option<String> {
        self.get_setting(key).as_str().map(ToString::to_string)
    }

    pub fn get_setting_bool(&self, key: &str, fallback: bool) -> bool {
        self.get_setting(key).as_bool().unwrap_or(fallback)
    }

    

    pub fn update_settings(&self, patch: &Value) -> Value {
        let mut data = self.lock();
        if !data["settings"].is_object() {
            data["settings"] = defaults()["settings"].clone();
        }
        if let (Some(settings), Some(patch_map)) = (
            data["settings"].as_object_mut(),
            patch.as_object(),
        ) {
            for (key, value) in patch_map {
                settings.insert(key.clone(), value.clone());
            }
        }
        let updated = data["settings"].clone();
        self.save_locked(&data);
        updated
    }

    pub fn array(&self, key: &str) -> Vec<Value> {
        self.get(key)
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    pub fn push_array(&self, key: &str, value: Value) {
        let mut data = self.lock();
        if !data[key].is_array() {
            data[key] = json!([]);
        }
        if let Some(list) = data[key].as_array_mut() {
            list.push(value);
        }
        self.save_locked(&data);
    }

    pub fn map_array<F>(&self, key: &str, mut mapper: F)
    where
        F: FnMut(&mut Vec<Value>),
    {
        let mut data = self.lock();
        if !data[key].is_array() {
            data[key] = json!([]);
        }
        if let Some(list) = data[key].as_array_mut() {
            mapper(list);
        }
        self.save_locked(&data);
    }

    pub fn set_language(&self, language: Option<String>) {
        self.set("language", match language {
            Some(lang) => Value::String(lang),
            None => Value::Null,
        });
    }

    pub fn set_active_account(&self, account_id: Option<String>) {
        self.set(
            "activeAccountId",
            match account_id {
                Some(id) => Value::String(id),
                None => Value::Null,
            },
        );
    }
}

