// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<D: LayoutDom> LayoutDom for SubtreeView<'_, D> {
    type NodeId = D::NodeId;

    fn document(&self) -> Self::NodeId {
        self.root
    }

    fn parent(&self, id: Self::NodeId) -> Option<Self::NodeId> {
        (id != self.root).then(|| self.dom.parent(id)).flatten()
    }

    fn prev_sibling(&self, id: Self::NodeId) -> Option<Self::NodeId> {
        (id != self.root)
            .then(|| self.dom.prev_sibling(id))
            .flatten()
    }

    fn next_sibling(&self, id: Self::NodeId) -> Option<Self::NodeId> {
        (id != self.root)
            .then(|| self.dom.next_sibling(id))
            .flatten()
    }

    fn dom_children(&self, id: Self::NodeId) -> impl Iterator<Item = Self::NodeId> + '_ {
        self.dom.dom_children(id)
    }

    fn kind(&self, id: Self::NodeId) -> NodeKind {
        self.dom.kind(id)
    }

    fn opaque_id(&self, id: Self::NodeId) -> u64 {
        self.dom.opaque_id(id)
    }

    fn element_name(&self, id: Self::NodeId) -> Option<&QualName> {
        self.dom.element_name(id)
    }

    fn attribute(&self, id: Self::NodeId, ns: &Namespace, local: &LocalName) -> Option<&str> {
        self.dom.attribute(id, ns, local)
    }

    fn attributes(&self, id: Self::NodeId) -> impl Iterator<Item = AttributeView<'_>> + '_ {
        self.dom.attributes(id)
    }

    fn text(&self, id: Self::NodeId) -> Option<&str> {
        self.dom.text(id)
    }
}
