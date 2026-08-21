use crate::application::analyzer::{AnalyzeRequest, AnalyzeResponse, AnalyzerService};
use crate::domain::errors::AppError;
use tauri::State;

#[tauri::command]
pub async fn analyze_url(
    service: State<'_, AnalyzerService>,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, AppError> {
    service.analyze(request).await
}
