use std::fs;
use std::io::Read;
use std::path::PathBuf;

use dirs;
use toml;
use failure::Error;
use failure::ResultExt;
use regex;
use regex::Regex;

#[derive(Clone, Debug, Deserialize)]
struct RawConfig {
    ignore: Vec<String>,
    ignores_delete: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub ignore: Vec<Regex>,
    pub ignores_delete: Vec<Regex>,
}

pub fn config() -> Result<Config, Error> {
    load_config()?.into_config()
}

fn find_config() -> Result<PathBuf, Error> {
    let mut tried = Vec::new();

    if let Some(mut config) = dirs::config_dir() {
        config.push("kill-desktop");
        fs::create_dir_all(&config)?;
        config.push("config.toml");
        if config.is_file() {
            return Ok(config);
        }

        tried.push(config);
    }

    if let Some(mut config) = dirs::home_dir() {
        config.push(".kill-desktop.toml");
        if config.is_file() {
            return Ok(config);
        }

        tried.push(config);
    }

    let config = PathBuf::from("kill-desktop.toml");
    if config.is_file() {
        return Ok(config);
    }

    tried.push(config);

    Err(format_err!(
        "couldn't find a config file, tried: {:?}",
        tried
    ))
}

fn load_config() -> Result<RawConfig, Error> {
    let path = find_config()?;
    let mut file = fs::File::open(&path).with_context(|_| format_err!("reading {:?}", path))?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(toml::from_slice(&bytes)?)
}

impl RawConfig {
    fn into_config(self) -> Result<Config, Error> {
        Ok(Config {
            ignore: self
                .ignore
                .into_iter()
                .map(|s| Regex::new(&s))
                .collect::<Result<Vec<_>, regex::Error>>()?,
            ignores_delete: self
                .ignores_delete
                .into_iter()
                .map(|s| Regex::new(&s))
                .collect::<Result<Vec<_>, regex::Error>>()?,
        })
    }
}
