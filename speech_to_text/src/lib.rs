use kinode_process_lib::{
    await_message, call_init, get_blob,
    http::{self, HttpClientAction, OutgoingHttpRequest},
    Address, Request, Response,
};
use std::{collections::HashMap, vec};
use stt_interface::{STTRequest, STTResponse};

pub const BASE_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

mod structs;
use structs::*;

wit_bindgen::generate!({
    path: "target/wit",
    world: "process-v0",
});

pub fn openai_whisper_request(audio_bytes: &[u8], openai_key: &str) {
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let content_type = format!("multipart/form-data; boundary={}", boundary);
    let headers = Some(HashMap::from_iter(vec![
        ("Content-Type".to_string(), content_type),
        (
            "Authorization".to_string(),
            format!("Bearer {}", openai_key),
        ),
    ]));
    let url = url::Url::parse(BASE_URL).unwrap();

    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.oga\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(audio_bytes);
    body.extend_from_slice(b"\r\n");

    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    body.extend_from_slice(b"whisper-1\r\n");

    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

    let _ = Request::to(("our", "http_client", "distro", "sys"))
        .body(
            serde_json::to_vec(&HttpClientAction::Http(OutgoingHttpRequest {
                method: http::Method::POST.to_string(),
                version: None,
                url: url.to_string(),
                headers: headers.unwrap_or_default(),
            }))
            .unwrap(),
        )
        .blob_bytes(body)
        .expects_response(30)
        .send();
}

pub fn openai_whisper_response() -> STTResponse {
    let Some(blob) = get_blob() else {
        return STTResponse::Error("Failed to get blob!".to_string());
    };
    let bytes = blob.bytes;
    match serde_json::from_slice::<WhisperResponse>(bytes.as_slice()) {
        Ok(response) => STTResponse::OpenaiTranscribed(response.text),
        Err(e) => STTResponse::Error(e.to_string()),
    }
}

fn register_openai_api_key(
    api_key: &str,
    state: &mut Option<State>,
) -> anyhow::Result<()> {
    match state {
        Some(_state) => {
            _state.openai_api_key = api_key.to_string();
            _state.save();
        }
        None => {
            let _state = State {
                openai_api_key: api_key.to_string(),
            };
            _state.save();
            *state = Some(_state);
        }
    }
    let _ = Response::new().body(serde_json::to_vec(&STTResponse::Ok)?).send();
    Ok(())
}

fn handle_message(state: &mut Option<State>) -> Option<()> {
    let msg = await_message().ok()?;
    let body = msg.body();

    if msg.is_request() {
        let Ok(stt_request) = serde_json::from_slice::<STTRequest>(&body) else {
            println!("Failed to parse STTRequest from message body");
            return None;
        };
        match stt_request {
            STTRequest::RegisterApiKey(key) => {
                let _ = register_openai_api_key(&key, state);
            },
            STTRequest::OpenaiTranscribe(audio_data) => {
                match state {
                    Some(state) => {
                        openai_whisper_request(&audio_data, &state.openai_api_key);
                    }
                    None => {
                        println!("No API key registered");
                    }
                }
            },
        }
    } else {
        let response = openai_whisper_response();
        let body = serde_json::to_vec(&response).ok()?;
        let _ = Response::new().body(body).send();
    }

    Some(())
}

// call_init!(init);
fn init(_our: Address) {
    let mut state = State::fetch();
    loop {
        handle_message(&mut state);
    }
}

// TODO: Zena: Proper multiform crate that avoids this, but that also works in WASM. 