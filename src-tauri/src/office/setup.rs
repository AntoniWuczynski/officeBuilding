//! The setup floor: its folder is the library, and it goes up on first run.

use super::Office;
use crate::model::ToolKind;
use crate::setup::{ItemKind, SetupView, Targets};

impl Office {
    /// Create the library and put up its floor, unless the human demolished it.
    pub fn ensure_setup_floor(&self) -> Result<(), String> {
        self.setup.ensure()?;
        let key = self.setup.dir().to_string_lossy().into_owned();
        let placed = {
            let inner = self.lock();
            let r = &inner.store.records;
            r.pinned_floors.contains(&key) || r.demolished_floors.contains_key(&key)
        };
        if placed {
            Ok(())
        } else {
            self.add_floor(&key).map(drop)
        }
    }

    /// Spawns a login shell to find the installed tools, so call it off the main thread.
    #[must_use]
    pub fn setup_view(&self) -> SetupView {
        let installed: Vec<ToolKind> = crate::tools::installed();
        self.setup.view(&self.home, &installed)
    }

    /// Copy one of `from`'s own items into the library.
    pub fn import_setup_item(
        &self,
        from: ToolKind,
        kind: ItemKind,
        name: &str,
    ) -> Result<SetupView, String> {
        self.setup.import(&self.home, from, kind, name)?;
        Ok(self.setup_view())
    }

    pub fn set_setup_targets(
        &self,
        kind: ItemKind,
        name: &str,
        targets: Targets,
    ) -> Result<SetupView, String> {
        self.setup.set_targets(kind, name, targets)?;
        Ok(self.setup_view())
    }
}
