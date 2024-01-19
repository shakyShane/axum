use crate::server_config::ServerConfig;
use std::fs::read_to_string;
use std::path::Path;

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
        pub items: Vec<crate::server_config::Route>,
    }

    let input = r#"
items:
  - path: /hello.js
    raw: "hello"
    cors: true
  - path: /hello.js
    json: ["2", "3"]
  - path: /node_modules
    dir: ./node_modules
  - path: /node_modules
    dir: ./node_modules
        
    "#;
    let c: Config = serde_yaml::from_str(input).unwrap();
    dbg!(c);
}

#[test]
fn test_deserialize_3() {
    use crate::server_config::Route;
    let input = r#"
path: /hello.js
dir: "hello"
cors: true
    "#;
    let c: Route = serde_yaml::from_str(input).unwrap();
    dbg!(c);
}
