//! The Manager group of Settings: which tool a floor's manager is, and its limits.

use super::{
    MANAGER_MAX_DESKS, MANAGER_SPEND_CEILING_USD, between, checked_names, clamp, clamped_name,
};
use serde::{Deserialize, Serialize};

/// The tools that can be a floor's manager.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagerTool {
    #[default]
    ClaudeCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManagerSettings {
    pub tool: ManagerTool,
    /// None: the tool's default model.
    pub model: Option<String>,
    /// None: the model's default effort.
    pub effort: Option<String>,
    /// Desks a manager may have hired at once: 1 to 10.
    pub max_desks: u32,
    /// Ceiling on the summed spend of the desks a manager hired, in whole US dollars: 1 to 1000.
    pub spend_ceiling_usd: u32,
}

impl Default for ManagerSettings {
    fn default() -> Self {
        Self {
            tool: ManagerTool::ClaudeCode,
            model: None,
            effort: None,
            max_desks: 3,
            spend_ceiling_usd: 20,
        }
    }
}

impl ManagerSettings {
    /// The same settings with each name trimmed (empty becomes None), or why one is not valid.
    pub(super) fn checked(mut self) -> Result<Self, String> {
        (self.model, self.effort) = checked_names(self.model, self.effort)?;
        if !MANAGER_MAX_DESKS.contains(&self.max_desks) {
            return Err(format!(
                "desks at once must be {}",
                between(&MANAGER_MAX_DESKS)
            ));
        }
        if !MANAGER_SPEND_CEILING_USD.contains(&self.spend_ceiling_usd) {
            return Err(format!(
                "the spend ceiling must be {} dollars",
                between(&MANAGER_SPEND_CEILING_USD)
            ));
        }
        Ok(self)
    }

    /// Clamp the numbers into range and drop a name that is not valid, as a file may hold any.
    pub(super) fn clamped(mut self) -> Self {
        self.max_desks = clamp(self.max_desks, &MANAGER_MAX_DESKS);
        self.spend_ceiling_usd = clamp(self.spend_ceiling_usd, &MANAGER_SPEND_CEILING_USD);
        self.model = clamped_name(self.model);
        self.effort = clamped_name(self.effort);
        self
    }
}
