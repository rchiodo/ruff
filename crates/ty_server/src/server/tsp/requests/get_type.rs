use std::borrow::Cow;

use anyhow::anyhow;
use lsp_types::{Url, request::Request};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::{
    Expr,
    visitor::{Visitor, walk_expr, walk_stmt},
};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;
use ty_project::ProjectDatabase;
use ty_python_semantic::{HasType, SemanticModel};

use crate::document::PositionExt;
use crate::server::tsp::protocol::{
    GetTypeParams, GetTypeResponse, Type, TypeCategory, TypeFlags, TypeHandle,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

// Define the TSP GetType request
#[allow(dead_code)]
pub(crate) struct GetTypeRequest;

impl Request for GetTypeRequest {
    type Params = GetTypeParams;
    type Result = GetTypeResponse;
    const METHOD: &'static str = "typeServer/getType";
}

pub(crate) struct GetTypeRequestHandler;

impl GetTypeRequestHandler {
    pub(crate) fn document_url(params: &GetTypeParams) -> Cow<'_, Url> {
        // Convert the URI string to a URL
        match Url::parse(&params.node.uri) {
            Ok(url) => Cow::Owned(url),
            Err(_) => {
                // If parsing fails, create a file URL as fallback
                match Url::from_file_path(&params.node.uri) {
                    Ok(url) => Cow::Owned(url),
                    Err(()) => {
                        // Last resort - create a dummy URL
                        Cow::Owned(Url::parse("file:///unknown").unwrap())
                    }
                }
            }
        }
    }

    pub(crate) fn handle_request(
        id: &lsp_server::RequestId,
        db: &ProjectDatabase,
        snapshot: &crate::session::DocumentSnapshot,
        client: &Client,
        params: &GetTypeParams,
    ) {
        let result = Self::run_with_snapshot(db, snapshot, client, params);

        if let Err(err) = &result {
            tracing::error!("An error occurred with request ID {id}: {err}");
            client.show_error_message("ty encountered a problem. Check the logs for more details.");
        }

        client.respond(id, result);
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: &GetTypeParams,
    ) -> crate::server::Result<GetTypeResponse> {
        let Some(file) = snapshot.file(db) else {
            return Err(crate::server::api::Error::new(
                anyhow!("Failed to resolve file"),
                lsp_server::ErrorCode::InternalError,
            ));
        };
        let source = source_text(db, file);
        let index = line_index(db, file);

        // Convert LSP position to text offset
        let start_position = lsp_types::Position {
            line: params.node.range.start.line,
            character: params.node.range.start.character,
        };

        let end_position = lsp_types::Position {
            line: params.node.range.end.line,
            character: params.node.range.end.character,
        };

        let start_offset =
            start_position.to_text_size(source.as_str(), &index, crate::PositionEncoding::UTF16);
        let end_offset =
            end_position.to_text_size(source.as_str(), &index, crate::PositionEncoding::UTF16);
        let range = TextRange::new(start_offset, end_offset);

        // Parse the module
        let parsed = parsed_module(db, file).load(db);

        // Find an expression at the given range using a simple visitor
        let mut finder = ExpressionFinder::new(range);
        finder.visit_body(&parsed.syntax().body);

        let ast_expr = finder.found_expression.ok_or_else(|| {
            crate::server::api::Error::new(
                anyhow!("No expression found at position"),
                lsp_server::ErrorCode::InvalidRequest,
            )
        })?;

        // Get the semantic index

        // Create a semantic model for this file
        let model = SemanticModel::new(db, file);

        // Get the type of the expression using HasType trait
        let semantic_type = ast_expr.inferred_type(&model);

        // Convert the semantic type to TSP type - we'll use Debug format for now
        let tsp_type = convert_semantic_type_to_tsp(db, format!("{:?}", semantic_type));

        Ok(tsp_type)
    }
}

/// Convert a semantic Type to a TSP Type
fn convert_semantic_type_to_tsp(
    _db: &ProjectDatabase,
    semantic_type: impl std::fmt::Display,
) -> Type {
    // Generate a unique handle for this type
    let type_str = semantic_type.to_string();
    let handle = TypeHandle::String(format!("type_{}", type_str.replace(" ", "_")));

    // Use the display string as the name
    let name = type_str.clone();

    // Determine the category and flags based on the type string
    // This is a simplified approach - in a real implementation you'd want
    // to properly analyze the semantic type
    let (category, flags, category_flags) = if name.contains("function") || name.contains("method")
    {
        (TypeCategory::Function, TypeFlags::CALLABLE, 0)
    } else if name.contains("class") {
        (TypeCategory::Class, TypeFlags::INSTANTIABLE, 0)
    } else if name.contains("module") {
        (TypeCategory::Module, TypeFlags::NONE, 0)
    } else if name.contains("|") {
        (TypeCategory::Union, TypeFlags::NONE, 0)
    } else if name.starts_with("Literal[") {
        (TypeCategory::Any, TypeFlags::LITERAL, 0)
    } else {
        (TypeCategory::Any, TypeFlags::NONE, 0)
    };

    Type {
        handle,
        name,
        category,
        flags,
        category_flags,
        alias_name: None,  // TODO: Extract alias information if available
        module_name: None, // TODO: Extract module information if available
        decl: None,        // TODO: Extract declaration information if available
    }
}

/// A simple visitor to find an expression that overlaps with a given range
struct ExpressionFinder<'a> {
    target_range: TextRange,
    found_expression: Option<&'a Expr>,
}

impl<'a> ExpressionFinder<'a> {
    fn new(target_range: TextRange) -> Self {
        Self {
            target_range,
            found_expression: None,
        }
    }

    fn visit_body(&mut self, body: &'a [ruff_python_ast::Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
            if self.found_expression.is_some() {
                break;
            }
        }
    }
}

impl<'a> Visitor<'a> for ExpressionFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        // If we already found an expression or this one doesn't overlap, skip
        if self.found_expression.is_some() {
            return;
        }

        // Check if this expression overlaps with our target range
        if expr.range().intersect(self.target_range).is_some() {
            self.found_expression = Some(expr);
            return;
        }

        // Continue visiting children
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'a ruff_python_ast::Stmt) {
        if self.found_expression.is_some() {
            return;
        }
        walk_stmt(self, stmt);
    }
}
