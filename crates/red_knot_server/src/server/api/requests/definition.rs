use std::borrow::Cow;

use lsp_types::request::GotoDefinition;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Url};
use red_knot_project::{Db, ProjectDatabase};
use red_knot_python_semantic::semantic_index::{semantic_index, symbol_table};
use red_knot_python_semantic::semantic_index::symbol::FileScopeId;
use ruff_db::parsed::{self, parsed_module};
use ruff_source_file::OneIndexed;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::DocumentSnapshot;
use red_knot_python_semantic::util::nodes::{find_node_and_owning_scope};
use ruff_db::source::{line_index, source_text};

pub(crate) struct DefinitionRequestHandler;

impl RequestHandler for DefinitionRequestHandler {
    type RequestType = GotoDefinition;
}

impl BackgroundDocumentRequestHandler for DefinitionRequestHandler {
    fn document_url(params: &GotoDefinitionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: ProjectDatabase,
        _notifier: Notifier,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(&db) else {
            tracing::info!(
                "No file found for snapshot for `{}`",
                snapshot.query().file_url()
            );
            return Ok(None);
        };

        let position = params.text_document_position_params.position;
        let line_index = line_index(&db, file);
        let source = source_text(&db, file);
        let parsed = parsed_module(&db, file);

        let mut locations = vec![];
        let index = semantic_index(&db, file);
        let offset = line_index.offset(
            OneIndexed::from_zero_indexed(position.line as usize),
            OneIndexed::from_zero_indexed(position.character as usize),
            source.as_str(),
        );
        let node = find_node_and_owning_scope(parsed, offset);

        // Find the symbol for the node.
        let node = node.ok_or_else(|| {
            tracing::info!("No node found for offset {}", offset);
            "No node found for offset"
        })?;

        let scope_id= index.child_scopes(scope)
        let symbol_table = symbol_table(db, node.scope);

        // for (file, range) in db.find_definitions(file, line_index, source, position) {
        //     let url = Url::from_file_path(file).unwrap();
        //     let location = Location {
        //         uri: url,
        //         range: range.into(),
        //     };
        //     locations.push(location);
        // }

        Ok(Some(GotoDefinitionResponse::Array(locations)))
    }
}

// fn get_symbol<'db>(
//     db: &'db Db,
//     scopes: &[&str],
//     symbol_name: &str,
// ) -> Symbol<'db> {
//     let file = system_path_to_file(db, file_name).expect("file to exist");
//     let index = semantic_index(db, file);
//     let mut file_scope_id = FileScopeId::global();
//     let mut scope = file_scope_id.to_scope_id(db, file);
//     for expected_scope_name in scopes {
//         file_scope_id = index
//             .child_scopes(file_scope_id)
//             .next()
//             .unwrap_or_else(|| panic!("scope of {expected_scope_name}"))
//             .0;
//         scope = file_scope_id.to_scope_id(db, file);
//         assert_eq!(scope.name(db), *expected_scope_name);
//     }

//     symbol(db, scope, symbol_name)
// }
