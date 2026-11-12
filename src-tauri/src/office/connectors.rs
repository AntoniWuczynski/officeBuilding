//! A floor's links to connectors: which outside tool feeds it, kept in the store, and the
//! hub that caches each linked floor's items.

use super::{Inner, Office};
use crate::connectors::{self, LinkedFloor, hub::Hub};
use crate::model::{Connection, ConnectorKind, Link};
use std::sync::Arc;

impl Office {
    /// Use `hub` for this office's connector items.
    #[must_use]
    pub fn with_connectors(mut self, hub: Arc<Hub>) -> Self {
        self.connectors = hub;
        self
    }

    /// Each standing floor's links with how their last check went.
    pub(super) fn connections(&self, inner: &Inner) -> Vec<Connection> {
        inner
            .found
            .roots
            .iter()
            .flat_map(|(project_id, root)| {
                let root = root.to_string_lossy();
                let links = inner.store.records.connectors.get(root.as_ref());
                links
                    .into_iter()
                    .flatten()
                    .map(|link| self.connectors.connection(project_id, &root, link))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The stored root and link behind a connector item's floor.
    pub(super) fn link_of(
        &self,
        project_id: &str,
        kind: ConnectorKind,
    ) -> Result<(String, Link), String> {
        let inner = self.lock();
        let root = Self::root_key(&inner, project_id)
            .map_err(|_| "that floor is no longer in the building".to_string())?;
        let link = inner
            .store
            .records
            .connectors
            .get(&root)
            .and_then(|links| links.iter().find(|l| l.kind() == kind))
            .cloned()
            .ok_or_else(|| format!("that floor is not connected to {} any more", kind.label()))?;
        Ok((root, link))
    }

    /// The page of a connector item at its source.
    pub fn todo_link(&self, todo_id: &str) -> Result<String, String> {
        let (project_id, kind, external_id) =
            connectors::parse_item_id(todo_id).ok_or("that item has no page to open")?;
        let (root, _) = self.link_of(project_id, kind)?;
        self.connectors
            .url(&root, kind, external_id)
            .ok_or_else(|| "that issue is no longer on this floor's list".to_string())
    }

    /// Feed a floor from a connector, replacing its link of the same kind. Pins the floor.
    pub fn connect_floor(&self, project_id: &str, link: Link) -> Result<(), String> {
        let link = connectors::checked(link)?;
        let mut inner = self.lock();
        let root = Self::root_key(&inner, project_id)?;
        let records = &mut inner.store.records;
        let links = records.connectors.entry(root.clone()).or_default();
        let kind = link.kind();
        let changed = !links.contains(&link);
        links.retain(|l| l.kind() != kind);
        links.push(link);
        if !records.pinned_floors.contains(&root) {
            records.pinned_floors.push(root.clone());
        }
        inner.store.save()?;
        drop(inner);
        if changed {
            self.connectors.forget(&root, kind);
        }
        Ok(())
    }

    /// Stop feeding a floor from a connector. Not being linked is not an error.
    pub fn disconnect_floor(&self, project_id: &str, kind: ConnectorKind) -> Result<(), String> {
        let mut inner = self.lock();
        let root = Self::root_key(&inner, project_id)?;
        let connectors = &mut inner.store.records.connectors;
        if let Some(links) = connectors.get_mut(&root) {
            links.retain(|l| l.kind() != kind);
            if links.is_empty() {
                connectors.remove(&root);
            }
        }
        inner.store.save()?;
        drop(inner);
        self.connectors.forget(&root, kind);
        Ok(())
    }

    /// Every standing floor that has a link.
    #[must_use]
    pub fn linked_floors(&self) -> Vec<LinkedFloor> {
        let inner = self.lock();
        inner
            .found
            .roots
            .iter()
            .filter_map(|(project_id, root)| {
                let root = root.to_string_lossy().into_owned();
                let links = inner.store.records.connectors.get(&root)?;
                (!links.is_empty()).then(|| LinkedFloor {
                    project_id: project_id.clone(),
                    root,
                    links: links.clone(),
                })
            })
            .collect()
    }

    /// The key a floor's records are stored under: its absolute repo root.
    pub(super) fn root_key(inner: &Inner, project_id: &str) -> Result<String, String> {
        inner
            .found
            .roots
            .get(project_id)
            .map(|root| root.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{project_id} is not a floor of this building"))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use super::*;
    use crate::connectors::fake::{FakeConnector, issue, listing};
    use crate::connectors::{Connector, ConnectorError};
    use crate::model::{Party, Project};
    use crate::store::Store;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    fn linear(label: Option<&str>) -> Link {
        Link::Linear {
            team_id: "team-1".to_string(),
            team_key: "ENG".to_string(),
            team_name: "Engineering".to_string(),
            project: None,
            label: label.map(str::to_string),
        }
    }

    fn root_of(dir: &std::path::Path) -> String {
        dir.join("home/code/app").to_string_lossy().into_owned()
    }

    #[test]
    fn connecting_stores_the_link_under_the_root_and_pins_the_floor() {
        let (office, dir, floor) = floor_office("conn-store");
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        let again = Store::open(dir.join("store.json")).expect("reopen");
        let root = root_of(&dir);
        assert_eq!(
            again.records.connectors.get(&root),
            Some(&vec![linear(None)])
        );
        assert!(again.records.pinned_floors.contains(&root));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn connecting_again_replaces_the_link() {
        let (office, dir, floor) = floor_office("conn-replace");
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        office
            .connect_floor(&floor.id, linear(Some("bug")))
            .expect("connect");
        let linked = office.linked_floors();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].links, vec![linear(Some("bug"))]);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn changing_the_link_drops_the_old_board_but_the_same_link_keeps_it() {
        let (office, dir, floor, _fake, _hub) = linked("conn-relink");
        office.connect_floor(&floor.id, linear(None)).expect("same");
        assert_eq!(office.snapshot().todos.len(), 3);
        office
            .connect_floor(&floor.id, linear(Some("bug")))
            .expect("changed");
        assert_eq!(office.snapshot().todos.len(), 1);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn an_unknown_floor_cannot_be_connected() {
        let (office, dir, _floor) = floor_office("conn-unknown");
        assert_eq!(
            office.connect_floor("p-nope", linear(None)),
            Err("p-nope is not a floor of this building".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn disconnecting_removes_the_link_keeps_the_pin_and_is_repeatable() {
        let (office, dir, floor) = floor_office("conn-off");
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        office
            .disconnect_floor(&floor.id, ConnectorKind::Linear)
            .expect("disconnect");
        office
            .disconnect_floor(&floor.id, ConnectorKind::Linear)
            .expect("again");
        assert!(office.linked_floors().is_empty());
        let again = Store::open(dir.join("store.json")).expect("reopen");
        assert!(again.records.connectors.is_empty());
        assert!(again.records.pinned_floors.contains(&root_of(&dir)));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn linked_floors_lists_standing_floors_only() {
        let (office, dir, floor) = floor_office("conn-linked");
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        office
            .lock()
            .store
            .records
            .connectors
            .insert("/gone/elsewhere".to_string(), vec![linear(None)]);
        let linked = office.linked_floors();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].project_id, floor.id);
        assert_eq!(linked[0].root, root_of(&dir));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    const NOW: &str = "2026-11-12T10:15:00Z";

    /// A floor with a TODO.md item, linked to a fake connector that listed issues 1 and 2.
    fn linked(tag: &str) -> (Office, PathBuf, Project, Arc<FakeConnector>, Arc<Hub>) {
        let (office, dir, floor) = floor_office(tag);
        let fake = Arc::new(FakeConnector::new(Duration::from_secs(60)));
        let connector: Arc<dyn Connector> = fake.clone();
        let hub = Arc::new(Hub::new(vec![connector], Box::new(|| {})));
        let office = office.with_connectors(Arc::clone(&hub));
        fs::write(dir.join("home/code/app/TODO.md"), "# TODO\n- [ ] fix it\n").expect("write");
        office.refresh();
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        fake.push_list(Ok(listing(vec![
            issue(1, Party::Human),
            issue(2, Party::Ai),
        ])));
        hub.poll(&office.linked_floors(), NOW);
        (office, dir, floor, fake, hub)
    }

    fn linear_id(floor: &Project, n: u32) -> String {
        format!("{}|linear|issue-{n}", floor.id)
    }

    fn done_of(office: &Office, id: &str) -> Option<bool> {
        office
            .snapshot()
            .todos
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.done)
    }

    #[test]
    fn the_snapshot_lists_file_items_then_connector_items_and_the_connection() {
        let (office, dir, floor, _fake, _hub) = linked("conn-snapshot");
        let snapshot = office.snapshot();
        let texts: Vec<&str> = snapshot.todos.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["fix it", "Issue 1", "Issue 2"]);
        assert_eq!(snapshot.todos[1].id, linear_id(&floor, 1));
        assert_eq!(snapshot.connections.len(), 1);
        let connection = &snapshot.connections[0];
        assert_eq!(connection.project_id, floor.id);
        assert_eq!(connection.link, linear(None));
        assert_eq!(connection.fetched_at.as_deref(), Some(NOW));
        assert_eq!(connection.error, None);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn an_office_without_connectors_behaves_as_before() {
        let (office, dir, floor) = floor_office("conn-none");
        fs::write(dir.join("home/code/app/TODO.md"), "# TODO\n- [ ] fix it\n").expect("write");
        office.refresh();
        let snapshot = office.snapshot();
        assert!(snapshot.connections.is_empty());
        assert_eq!(snapshot.todos.len(), 1);
        office
            .connect_floor(&floor.id, linear(None))
            .expect("connect");
        let snapshot = office.snapshot();
        assert_eq!(snapshot.todos.len(), 1);
        assert_eq!(snapshot.connections.len(), 1);
        assert_eq!(snapshot.connections[0].fetched_at, None);
        assert_eq!(
            office.tick_todo(&linear_id(&floor, 1)),
            Err("that connector is not available".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn ticking_reaches_the_connector_or_the_file() {
        let (office, dir, floor, fake, _hub) = linked("conn-tick");
        office.tick_todo(&linear_id(&floor, 2)).expect("tick");
        assert_eq!(fake.ticked(), vec![(linear(None), "issue-2".to_string())]);
        assert_eq!(done_of(&office, &linear_id(&floor, 2)), Some(true));
        let file_id = office.snapshot().todos[0].id.clone();
        office.tick_todo(&file_id).expect("tick");
        assert!(
            fs::read_to_string(dir.join("home/code/app/TODO.md"))
                .expect("read")
                .contains("- [x] fix it")
        );
        assert_eq!(fake.ticked().len(), 1);
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn a_refused_connector_tick_says_why_and_leaves_the_item_open() {
        let (office, dir, floor, fake, _hub) = linked("conn-refused");
        fake.set_tick(Err(ConnectorError::Refused(
            "Linear said: no access".to_string(),
        )));
        assert_eq!(
            office.tick_todo(&linear_id(&floor, 1)),
            Err("Linear said: no access".to_string())
        );
        assert_eq!(done_of(&office, &linear_id(&floor, 1)), Some(false));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn todo_link_gives_the_cached_url_only() {
        let (office, dir, floor, _fake, _hub) = linked("conn-url");
        assert_eq!(
            office.todo_link(&linear_id(&floor, 1)),
            Ok("https://linear.app/example/issue/ENG-1".to_string())
        );
        let file_id = office.snapshot().todos[0].id.clone();
        assert_eq!(
            office.todo_link(&file_id),
            Err("that item has no page to open".to_string())
        );
        assert_eq!(
            office.todo_link(&linear_id(&floor, 9)),
            Err("that issue is no longer on this floor's list".to_string())
        );
        assert_eq!(
            office.todo_link("p-nope|linear|issue-1"),
            Err("that floor is no longer in the building".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn disconnecting_drops_the_connector_items() {
        let (office, dir, floor, _fake, _hub) = linked("conn-drop");
        office
            .disconnect_floor(&floor.id, ConnectorKind::Linear)
            .expect("disconnect");
        let snapshot = office.snapshot();
        assert!(snapshot.connections.is_empty());
        assert_eq!(snapshot.todos.len(), 1);
        assert_eq!(
            office.todo_link(&linear_id(&floor, 1)),
            Err("that floor is not connected to Linear any more".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }
}
