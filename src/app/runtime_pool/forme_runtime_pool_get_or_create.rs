// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl FormeRuntimePool {
    pub fn get_or_create(&mut self, graph: GraphId, forme: FormeRef) -> &mut FormeRuntime {
        let key = FormeRuntimeKey { graph, forme };
        self.runtimes.entry(key).or_insert_with(|| FormeRuntime {
            key,
            workbench: Workbench::new(),
        })
    }

    pub fn get(&self, graph: GraphId, forme: FormeRef) -> Option<&FormeRuntime> {
        self.runtimes.get(&FormeRuntimeKey { graph, forme })
    }

    pub fn get_mut(&mut self, graph: GraphId, forme: FormeRef) -> Option<&mut FormeRuntime> {
        self.runtimes.get_mut(&FormeRuntimeKey { graph, forme })
    }

    pub fn iter(&self) -> impl Iterator<Item = &FormeRuntime> {
        self.runtimes.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut FormeRuntime> {
        self.runtimes.values_mut()
    }

    pub fn len(&self) -> usize {
        self.runtimes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runtimes.is_empty()
    }
}
