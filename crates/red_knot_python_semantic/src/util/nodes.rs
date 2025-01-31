use std::any::Any;

use ruff_db::parsed::ParsedModule;
use ruff_python_ast::{
    visitor::source_order::{walk_body, SourceOrderVisitor, TraversalSignal},
    AnyNodeRef,
};
use ruff_text_size::{Ranged, TextSize};

use crate::node_key::NodeKey;

pub fn find_node_key(parsed_module: &ParsedModule, offset: TextSize) -> Option<NodeKey> {
    let mut visitor = SearchAstVisitor::new(offset);
    let module = parsed_module.syntax();
    walk_body(&mut visitor, &module.body);
    visitor.result
}

#[derive(Debug)]
struct SearchAstVisitor {
    result: Option<NodeKey>,
    offset: TextSize,
}

impl SearchAstVisitor {
    fn new(offset: TextSize) -> Self {
        Self {
            result: None,
            offset,
        }
    }
}

impl<'ast> SourceOrderVisitor<'ast> for SearchAstVisitor {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        if node.range().contains(self.offset) {
            // Keep narrowing as we go deeper into the tree.
            self.result = Some(NodeKey::from_node(node));
        }
        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, _node: AnyNodeRef<'ast>) {}
}
