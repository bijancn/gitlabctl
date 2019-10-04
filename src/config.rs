use dirs::home_dir;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct Config {
    server: String,
    access_token: String,
}

impl Config {
    pub fn parse_from_disk() -> Config {
        let config_path = home_dir()
            .expect("Could not find home dir")
            .join(".config/gitlab.toml");
        let config_string = fs::read_to_string(&config_path)
            .expect(format!("Something went wrong reading the file {:?}", &config_path).as_str());

        toml::from_str(&config_string).expect("Could not parse the config")
    }

    pub fn server(&self) -> &String {
        &self.server
    }

    pub fn access_token(&self) -> &String {
        &self.access_token
    }
}
