use std::borrow::Cow;

use anyhow::anyhow;
use lsp_types::{Url, request::Request};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text};
use ruff_text_size::TextRange;
use ty_project::ProjectDatabase;

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
        let _range = TextRange::new(start_offset, end_offset);

        // Parse the module for basic validation
        let _parsed = parsed_module(db, file).load(db);

        // For now, return a simple type response
        // TODO: Implement proper type resolution using semantic analysis
        let tsp_type = Type {
            handle: TypeHandle::String("unknown".to_string()),
            name: "Unknown".to_string(),
            category: TypeCategory::Any,
            flags: TypeFlags::NONE,
            category_flags: 0,
            alias_name: None,
            module_name: None,
            decl: None,
        };

        Ok(tsp_type)
    }
}
