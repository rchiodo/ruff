use std::borrow::Cow;

use lsp_types::{Url, request::Request};
use ty_project::ProjectDatabase;

use crate::server::tsp::protocol::{GetTypeParams, GetTypeResponse};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

use super::common::TspCommon;

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
        TspCommon::document_url(&params.node.uri)
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
        // Find expression at the given range
        let ast_expr = TspCommon::find_expression_at_range(
            db,
            snapshot,
            &params.node.uri,
            &params.node.range,
        )?;

        // Get the semantic type for the expression
        let semantic_type = TspCommon::get_semantic_type_for_expression(db, snapshot, &ast_expr)?;

        // Convert the semantic type to TSP type with user-friendly names
        let tsp_type = TspCommon::convert_semantic_type_to_tsp(db, &semantic_type);

        Ok(tsp_type)
    }
}
