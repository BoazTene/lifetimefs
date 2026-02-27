use serde::{Serialize, Deserialize};
use serde_json::Value;


#[derive(Serialize, Deserialize, Debug)]
pub struct Command {
    pub action: String,
    pub params: Value
}

pub enum CommandActions {
    Mount,
}

impl CommandActions {
    pub fn to_string(&self) -> String {
        match self {
            CommandActions::Mount => "mount".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MountCommand {
    pub mountpoint: String
}

