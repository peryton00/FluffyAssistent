use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Command {
    KillProcess { pid: u32 },
    RequestCleanup,
    OpenPath { path: String },

    // Confirmation flow
    Confirm { command_id: String },
    Cancel { command_id: String },
}
