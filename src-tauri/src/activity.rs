//! Local process metadata only: no command lines, environment, or conversation content.
use std::collections::{HashMap, HashSet};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub struct Detector {
    system: System,
}
impl Detector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }
    pub fn count(&mut self) -> Option<usize> {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        let own = sysinfo::get_current_pid().ok()?;
        let user = self.system.process(own)?.user_id()?;
        let parents: HashMap<u32, Option<u32>> = self
            .system
            .processes()
            .iter()
            .map(|(pid, p)| (pid.as_u32(), p.parent().map(|id| id.as_u32())))
            .collect();
        let candidates: HashSet<u32> = self
            .system
            .processes()
            .iter()
            .filter(|(_, p)| {
                p.user_id() == Some(user)
                    && matches!(
                        p.name().to_string_lossy().to_ascii_lowercase().as_str(),
                        "codex" | "codex.exe" | "codex-cli" | "codex-cli.exe"
                    )
            })
            .map(|(pid, _)| pid.as_u32())
            .collect();
        let mut monitors: HashSet<u32> = self
            .system
            .processes()
            .iter()
            .filter(|(_, p)| {
                matches!(
                    p.name().to_string_lossy().to_ascii_lowercase().as_str(),
                    "chatgpt-quota-monitor" | "chatgpt-quota-monitor.exe" | "chatgpt-quota-mo"
                )
            })
            .map(|(pid, _)| pid.as_u32())
            .collect();
        monitors.insert(own.as_u32());
        Some(count_roots(&parents, &candidates, &monitors))
    }
}
fn count_roots(
    parents: &HashMap<u32, Option<u32>>,
    candidates: &HashSet<u32>,
    monitors: &HashSet<u32>,
) -> usize {
    candidates
        .iter()
        .filter(|pid| {
            let mut cursor = **pid;
            let mut seen = HashSet::new();
            while seen.insert(cursor) {
                if monitors.contains(&cursor) {
                    return false;
                }
                let Some(Some(parent)) = parents.get(&cursor) else {
                    return true;
                };
                if *parent != **pid && candidates.contains(parent) {
                    return false;
                }
                cursor = *parent;
            }
            false
        })
        .count()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_monitor_tree_and_collapses_helpers() {
        let parents = HashMap::from([
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (10, None),
            (11, Some(10)),
            (20, None),
            (30, None),
            (31, Some(30)),
        ]);
        assert_eq!(
            count_roots(
                &parents,
                &HashSet::from([2, 3, 10, 11, 20, 31]),
                &HashSet::from([1, 30])
            ),
            2
        );
        assert_eq!(
            count_roots(&parents, &HashSet::from([2, 3]), &HashSet::from([1])),
            0
        );
    }
    #[test]
    fn parent_cycle_is_bounded() {
        assert_eq!(
            count_roots(
                &HashMap::from([(2, Some(3)), (3, Some(2))]),
                &HashSet::from([2]),
                &HashSet::from([1])
            ),
            0
        );
    }
}
