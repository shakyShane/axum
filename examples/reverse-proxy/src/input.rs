use crate::server_config::{Content, RawContent, Route, ServerConfig};
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Input {
    pub servers: Vec<crate::server_config::ServerConfig>,
}

impl Input {
    pub fn from_yaml_path<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let str = read_to_string(path)?;
        let output = serde_yaml::from_str::<Self>(str.as_str())?;
        Ok(output)
    }
}

#[test]
fn test_deserialize() {
    let input = include_str!("../fixtures/input.yml");
    let _: Input = serde_yaml::from_str(input).unwrap();
}
#[test]
fn test_serialize() {
    let input = Input {
        servers: vec![ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            routes: vec![Route {
                path: PathBuf::from("/"),
                content: Content::Raw {
                    raw: RawContent::Html {
                        html: "html content".into(),
                    },
                },
            }],
        }],
    };
    let yaml = serde_yaml::to_string(&input).unwrap();
    println!("{}", yaml);
}
