use std::any::Any;

use ruff_db::parsed::ParsedModule;
use ruff_python_ast::{
    visitor::source_order::{walk_body, walk_module, SourceOrderVisitor, TraversalSignal},
    AnyNodeRef, ModModule, NodeKind,
};
use ruff_text_size::{Ranged, TextSize};

use crate::{
    ast_node_ref::AstNodeRef,
    node_key::NodeKey,
    semantic_index::symbol::{NodeWithScopeKind, NodeWithScopeRef},
};

pub struct NodeWithOwningScope<'a> {
    pub node: AnyNodeRef<'a>,
    pub scope: NodeWithScopeKind,
}

pub fn find_node_and_owning_scope(
    module: &ParsedModule,
    offset: TextSize,
) -> Option<NodeWithOwningScope> {
    let mut visitor = SearchAstVisitor::new(module, offset);
    walk_body(&mut visitor, module.suite());
    visitor.result?;
    let node = visitor.result.unwrap();
    let scope: NodeWithScopeKind = visitor.parent_scope.unwrap();
    Some(NodeWithOwningScope { node, scope })
}

#[derive(Debug)]
struct SearchAstVisitor<'a, 'b> {
    result: Option<AnyNodeRef<'a>>,
    parent_scope: Option<NodeWithScopeKind>,
    offset: TextSize,
    last_scope: Option<NodeWithScopeKind>,
    module: &'b ParsedModule,
}

impl<'a, 'b> SearchAstVisitor<'a, 'b> {
    fn new(module: &'b ParsedModule, offset: TextSize) -> Self {
        Self {
            result: None,
            parent_scope: None,
            last_scope: None,
            module,
            offset,
        }
    }
}

impl<'a, 'b> SourceOrderVisitor<'a> for SearchAstVisitor<'a, 'b> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        if node.range().contains(self.offset) {
            // Keep narrowing as we go deeper into the tree.
            self.result = Some(node);
            self.parent_scope = self.last_scope.clone();
        }
        if let Some(scope) = to_node_with_scope_kind(self.module, node) {
            self.last_scope = Some(scope);
        }
        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, _node: AnyNodeRef<'a>) {}
}

fn to_node_with_scope_kind(
    module: &ParsedModule,
    node: AnyNodeRef<'_>,
) -> Option<NodeWithScopeKind> {
    match node.kind() {
        NodeKind::ModModule => {
            // The `Module` variant of `NodeWithScopeKind` doesn't store
            // any data for a ModModule node, so we can return it directly.
            Some(NodeWithScopeKind::Module)
        }
        NodeKind::StmtClassDef => {
            // Example for a class node (which does store the reference):
            #[allow(unsafe_code)]
            node.as_stmt_class_def().map(|class_ref| {
                NodeWithScopeKind::Class(unsafe { AstNodeRef::new(module.clone(), class_ref) })
            })
        }
        NodeKind::StmtFunctionDef =>
        {
            #[allow(unsafe_code)]
            node.as_stmt_function_def().map(|func_ref| {
                NodeWithScopeKind::Function(unsafe { AstNodeRef::new(module.clone(), func_ref) })
            })
        }
        _ => None,
    }
}
