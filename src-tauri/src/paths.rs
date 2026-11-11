use std::fs;
use std::path::PathBuf;

pub struct AppPaths {
    pub app_data: PathBuf,
    pub resource_dir: PathBuf,
}

impl AppPaths {
    pub fn new(app_data: PathBuf, resource_dir: PathBuf) -> Self {
        AppPaths {
            app_data,
            resource_dir,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.app_data.join("buran-launcher-config.json")
    }

    pub fn secrets_key(&self) -> PathBuf {
        self.app_data.join("secrets.key")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.app_data.join("logs")
    }

    pub fn minecraft_root(&self) -> PathBuf {
        self.app_data.join(".minecraft")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.minecraft_root().join("versions")
    }

    pub fn instances_root(&self) -> PathBuf {
        let dir = self.app_data.join("instances");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.instances_root().join(instance_id)
    }

    pub fn servers_root(&self) -> PathBuf {
        let dir = self.app_data.join("servers");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn server_dir(&self, server_id: &str) -> PathBuf {
        self.servers_root().join(server_id)
    }

    pub fn java_root(&self) -> PathBuf {
        self.app_data.join("java")
    }

    pub fn java_install_dir(&self, major: u32) -> PathBuf {
        let dir = self.java_root().join(format!("jdk-{major}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn backgrounds_dir(&self) -> PathBuf {
        let dir = self.app_data.join("backgrounds");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn default_background(&self) -> Option<PathBuf> {
        let candidates = [
            self.resource_dir.join("assets/backgrounds/default.jpg"),
            self.app_data.join("assets/backgrounds/default.jpg"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/backgrounds/default.jpg"),
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("assets/backgrounds/default.jpg"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }

    pub fn ensure_instance_folders(&self, instance_id: &str) -> PathBuf {
        let base = self.instance_dir(instance_id);
        for sub in ["mods", "resourcepacks", "shaderpacks", "saves", "config"] {
            let _ = fs::create_dir_all(base.join(sub));
        }
        base
    }

    pub fn version_json_path(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id).join(format!("{id}.json"))
    }

    }