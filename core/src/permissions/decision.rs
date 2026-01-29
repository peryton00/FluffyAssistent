use serde::Serialize;

#[derive(Serialize, Debug)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    RequireConfirmation { reason: String },
}
