use std::sync::LazyLock;

pub const DEFAULT_CONFIG_FILE_PATH: &str = "config.toml";
pub static CONFIG_FILE_PATH: LazyLock<String> = LazyLock::new(set_config_file_name);
pub mod prod {
    pub const APP_ADDRESS: &str = "0.0.0.0:8000";
}

pub mod test {
    pub const APP_ADDRESS: &str = "127.0.0.1:0";
}

pub mod env {
    pub const CONFIG_FILE_NAME: &str = "CONFIG_FILE_NAME";
}

fn set_config_file_name() -> String {
    dotenv::dotenv().ok();
    std::env::var(env::CONFIG_FILE_NAME).unwrap_or(DEFAULT_CONFIG_FILE_PATH.to_string())
}
