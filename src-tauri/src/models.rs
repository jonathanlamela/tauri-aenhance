use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeAudioRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeAudioResponse {
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub frames: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAudioRequest {
    pub input_path: String,
    pub output_path: String,
    pub output_format: String,
    #[serde(default)]
    pub normalize: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAudioResponse {
    pub output_path: String,
    pub output_format: String,
    pub ffmpeg_used: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProgressPayload {
    pub task: String,
    pub stage: String,
    pub percent: u8,
}
