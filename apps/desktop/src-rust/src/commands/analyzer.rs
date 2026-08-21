use crate::application::analyzer::{AnalyzeRequest, AnalyzeResponse, AnalyzerService};
use crate::domain::errors::AppError;

pub async fn analyze_url_core(
    service: &AnalyzerService,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, AppError> {
    service.analyze(request).await
}
