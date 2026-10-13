use crate::{
    middleware::auth,
    models::error::AppError,
    services::product_exports::{ExportArtifact, ExportRequest, ResultExportService},
};
use actix_web::{HttpRequest, HttpResponse, web};

pub async fn export(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ExportRequest>,
    service: web::Data<ResultExportService>,
) -> Result<HttpResponse, AppError> {
    let actor = auth::authenticate_request(&req).await?;
    let artifact = service.export(&actor, &path, body.into_inner()).await?;
    response(artifact)
}

pub(super) fn response(artifact: ExportArtifact) -> Result<HttpResponse, AppError> {
    let mut response = HttpResponse::Ok();
    response
        .insert_header(("Content-Type", artifact.content_type()))
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", artifact.filename()),
        ))
        .insert_header(("Cache-Control", "no-store"))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .insert_header(("X-Result-Row-Count", artifact.row_count.to_string()))
        .insert_header(("X-Source-Version", artifact.source_version.to_string()));
    if let Some(csv) = &artifact.csv_encoding {
        response.insert_header((
            "X-Dts-Csv-Encoding",
            serde_json::to_string(csv).map_err(|error| AppError::Internal(error.to_string()))?,
        ));
    }
    Ok(response.body(artifact.into_bytes()))
}
