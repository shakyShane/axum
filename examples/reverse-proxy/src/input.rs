use crate::server_config::{Route, ServerConfig};
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Input {
    pub servers: Vec<ServerConfig>,
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
fn test_deserialize_2() {
    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    struct Config {
        pub items: Vec<Item>,
    }

    #[derive(serde::Deserialize, serde::Serialize, Debug)]
    #[serde(untagged)]
    enum Item {
        Raw { path: String, raw: String },
        Dir { path: String, dir: String },
        Html { path: String, html: String },
    }

    let input = r#"
items:
  - path: /hello.js
    raw: "hello"
  - path: /node_modules
    dir: ./node_modules
  - path: /node_modules
    dir: ./node_modules
        
    "#;
    let c: Config = serde_yaml::from_str(input).unwrap();
    dbg!(c);
}

#[test]
fn test_serialize() {
    let input = Input {
        servers: vec![ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            routes: vec![
                Route {
                    path: PathBuf::from("/assets"),
                    content: Content::Dir {
                        dir: "assets".into(),
                    },
                },
                Route {
                    path: PathBuf::from("/"),
                    content: Content::Raw {
                        raw: RawContent::Html {
                            html: "html content".into(),
                        },
                    },
                },
            ],
        }],
    };
    let yaml = serde_yaml::to_string(&input).unwrap();
    println!("{}", yaml);
}
