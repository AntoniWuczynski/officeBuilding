//! A floor's TODO lists: its own file or store items, and ticking one.

use super::{Inner, Office};
use crate::connectors;
use crate::model::{Party, TodoItem};
use crate::project_files;

impl Office {
    /// A floor's TODOs for one assignee: its file when it has one, else the app's store.
    pub(super) fn own_todos(inner: &Inner, project_id: &str, assignee: Party) -> Vec<TodoItem> {
        let from_file = inner.files.get(project_id).and_then(|f| match assignee {
            Party::Human => f.human_todos.clone(),
            Party::Ai => f.agent_todos.clone(),
        });
        from_file.unwrap_or_else(|| {
            inner
                .store
                .records
                .todos
                .iter()
                .filter(|t| t.project_id == project_id && t.assignee == assignee)
                .cloned()
                .collect()
        })
    }

    /// Every TODO in the building, floor by floor: its own items, then its connectors' items.
    pub(super) fn all_todos(&self, inner: &Inner) -> Vec<TodoItem> {
        inner
            .found
            .projects
            .iter()
            .flat_map(|p| {
                let mut all = Self::own_todos(inner, &p.id, Party::Human);
                all.extend(Self::own_todos(inner, &p.id, Party::Ai));
                if let Some(root) = inner.found.roots.get(&p.id) {
                    all.extend(self.connectors.todos(&p.id, &root.to_string_lossy()));
                }
                all
            })
            .collect()
    }

    pub fn tick_todo(&self, todo_id: &str) -> Result<(), String> {
        if let Some((project_id, kind, external_id)) = connectors::parse_item_id(todo_id) {
            // The Office lock is released before the hub calls the connector.
            let (root, link) = self.link_of(project_id, kind)?;
            return self.connectors.tick(&root, &link, external_id);
        }
        let from_file = {
            let inner = self.lock();
            inner
                .files
                .values()
                .flat_map(|f| f.human_todos.iter().chain(f.agent_todos.iter()).flatten())
                .find(|t| t.id == todo_id)
                .map(|t| {
                    (
                        t.project_id.clone(),
                        project_files::parse_item_id(todo_id)
                            .map(|(file, line)| (file.to_string(), line)),
                    )
                })
        };
        if let Some((project_id, location)) = from_file {
            let (file, line) = location.ok_or("malformed todo id")?;
            let root = self
                .lock()
                .found
                .roots
                .get(&project_id)
                .cloned()
                .ok_or("that floor is no longer in the building")?;
            project_files::tick(&root, &file, line)?;
            self.refresh();
            return Ok(());
        }
        let mut inner = self.lock();
        let todo = inner
            .store
            .records
            .todos
            .iter_mut()
            .find(|t| t.id == todo_id)
            .ok_or_else(|| format!("unknown todo: {todo_id}"))?;
        todo.done = true;
        inner.store.save()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::floor_office;
    use crate::model::{Party, TodoItem, TodoSource};
    use std::fs;

    #[test]
    fn a_floor_file_shows_its_items_with_a_file_source() {
        let (office, dir, floor) = floor_office("todos-file");
        fs::write(dir.join("home/code/app/TODO.md"), "# TODO\n- [ ] fix it\n").expect("write");
        office.refresh();
        let todos = office.snapshot().todos;
        let item = todos
            .iter()
            .find(|t| t.project_id == floor.id)
            .expect("item");
        assert_eq!(item.text, "fix it");
        assert_eq!(
            item.source,
            TodoSource::File {
                file: "TODO.md".to_string()
            }
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn ticking_a_file_item_writes_the_file_and_shows_it_done() {
        let (office, dir, floor) = floor_office("todos-tick-file");
        let file = dir.join("home/code/app/TODO.md");
        fs::write(&file, "# TODO\n- [ ] fix it\n").expect("write");
        office.refresh();
        let id = office.snapshot().todos[0].id.clone();
        office.tick_todo(&id).expect("tick");
        assert!(
            fs::read_to_string(&file)
                .expect("read")
                .contains("- [x] fix it")
        );
        let todos = office.snapshot().todos;
        assert!(todos.iter().all(|t| t.project_id == floor.id && t.done));
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn ticking_a_store_item_marks_it_in_the_store() {
        let (office, dir, floor) = floor_office("todos-tick-store");
        {
            let mut inner = office.lock();
            inner.store.records.todos.push(TodoItem {
                id: "t-1".to_string(),
                project_id: floor.id.clone(),
                text: "own".to_string(),
                done: false,
                assignee: Party::Human,
                source: TodoSource::App,
            });
        }
        office.tick_todo("t-1").expect("tick");
        assert!(
            office
                .snapshot()
                .todos
                .iter()
                .any(|t| t.id == "t-1" && t.done)
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn an_unknown_id_is_refused() {
        let (office, dir, _floor) = floor_office("todos-unknown");
        assert_eq!(
            office.tick_todo("nope"),
            Err("unknown todo: nope".to_string())
        );
        fs::remove_dir_all(dir).expect("cleanup");
    }
}
