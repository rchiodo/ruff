use std::borrow::Cow;

use lsp_types::request::GotoDefinition;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Url};
use red_knot_project::{Db, ProjectDatabase};
use red_knot_python_semantic::semantic_index::semantic_index;
use ruff_db::parsed::{self, parsed_module};
use ruff_source_file::OneIndexed;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::DocumentSnapshot;
use red_knot_python_semantic::util::nodes::find_node_key;
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
        let sem_index = semantic_index(&db, file);
        let offset = line_index.offset(
            OneIndexed::from_zero_indexed(position.line as usize),
            OneIndexed::from_zero_indexed(position.character as usize),
            source.as_str(),
        );
        let node_key = find_node_key(parsed, offset);
        sem_index.definition(node_key);

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
