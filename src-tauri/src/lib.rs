mod audio;
mod models;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use audio::{
    analyze_audio_with_progress, boost_volume as boost_volume_impl, ensure_output_path,
    process_audio_with_progress, read_audio_bytes as read_audio_bytes_impl,
};
use models::{
    AnalyzeAudioRequest, AnalyzeAudioResponse, AudioProgressPayload, ProcessAudioRequest,
    ProcessAudioResponse,
};
use tauri::Emitter;

struct CancelFlag(Arc<AtomicBool>);

#[tauri::command]
async fn analyze_audio(
    app: tauri::AppHandle,
    request: AnalyzeAudioRequest,
) -> Result<AnalyzeAudioResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        analyze_audio_with_progress(&request.path, |stage, percent| {
            let _ = app.emit(
                "audio-progress",
                AudioProgressPayload {
                    task: "analyze".to_string(),
                    stage: stage.to_string(),
                    percent,
                },
            );
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn read_audio_bytes(path: String) -> Result<String, String> {
    read_audio_bytes_impl(&path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn boost_volume(path: String, gain_db: f32) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        boost_volume_impl(&path, gain_db).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn cancel_process(state: tauri::State<CancelFlag>) {
    state.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
async fn process_audio(
    app: tauri::AppHandle,
    state: tauri::State<'_, CancelFlag>,
    mut request: ProcessAudioRequest,
) -> Result<ProcessAudioResponse, String> {
    request.output_path = ensure_output_path(&request.output_path, &request.output_format)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string();

    // Reset cancel flag before starting
    state.0.store(false, Ordering::Relaxed);
    let cancel = Arc::clone(&state.0);

    tauri::async_runtime::spawn_blocking(move || {
        process_audio_with_progress(
            request,
            |stage, percent| {
                let _ = app.emit(
                    "audio-progress",
                    AudioProgressPayload {
                        task: "process".to_string(),
                        stage: stage.to_string(),
                        percent,
                    },
                );
            },
            &cancel,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(CancelFlag(Arc::new(AtomicBool::new(false))))
        .invoke_handler(tauri::generate_handler![
            analyze_audio,
            read_audio_bytes,
            process_audio,
            cancel_process,
            boost_volume
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
