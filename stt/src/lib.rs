use std::collections::HashMap;

use crate::kinode::process::stt::{SttRequest, SttResponse};
use kinode_process_lib::{
    await_message, call_init, get_blob, println, Address, Message, Request,
    Response, http::{self, HttpClientAction, OutgoingHttpRequest},
};

pub const BASE_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

mod structs;
use structs::*;

wit_bindgen::generate!({
    path: "target/wit",
    world: "stt-appattacc-dot-os-v0",
    generate_unused_types: true,
    additional_derives: [serde::Deserialize, serde::Serialize, process_macros::SerdeJsonInto],
});

// TODO: Zena: CBA to find a crate that does this in rust-wasm-wasi, will just rewrite this process in js once support is there.
// This works, it's just ugly. 
pub fn openai_whisper_request(audio_bytes: &[u8], openai_key: &str) -> anyhow::Result<()> {
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

    Request::to(("our", "http_client", "distro", "sys"))
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
        .send()
}

fn register_openai_api_key(api_key: &str, state: &mut Option<State>) -> anyhow::Result<()> {
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
    let _ = Response::new()
        .body(serde_json::to_vec(&SttResponse::RegisterApiKey(Ok(
            "Success".to_string(),
        )))?)
        .send();
    Ok(())
}

fn handle_message(state: &mut Option<State>) -> anyhow::Result<()> {
    let msg = await_message()?;
    match msg {
        Message::Request { body, .. } => handle_request(state, &body),
        Message::Response { .. } => {
            // No need for context numbers, as there is only one response type to handle.
            handle_openai_whisper_response()
        }
    }
}

fn handle_request(state: &mut Option<State>, body: &[u8]) -> anyhow::Result<()> {
    let stt_request = serde_json::from_slice::<SttRequest>(&body)?;
    match stt_request {
        SttRequest::RegisterApiKey(key) => {
            return register_openai_api_key(&key, state);
        }
        SttRequest::OpenaiTranscribe(audio_data) => match state {
            Some(state) => return openai_whisper_request(&audio_data, &state.openai_api_key),
            None => {
                return Err(anyhow::anyhow!("No API key registered"));
            }
        },
    }
}

pub fn handle_openai_whisper_response() -> anyhow::Result<()> {
    let Some(blob) = get_blob() else {
        return Err(anyhow::anyhow!("Failed to get blob!"));
    };

    let bytes = blob.bytes;
    let response = match serde_json::from_slice::<WhisperResponse>(bytes.as_slice()) {
        Ok(response) => SttResponse::OpenaiTranscribe(Ok(response.text)),
        Err(e) => {
            let error_message = e.to_string();
            match String::from_utf8(bytes.to_vec()) {
                Ok(decoded) => SttResponse::OpenaiTranscribe(Err(format!("{}: {}", error_message, decoded))),
                Err(_) => SttResponse::OpenaiTranscribe(Err(error_message)),
            }
        },
    };

    let body = serde_json::to_vec(&response)?;
    Response::new().body(body).send()
}

call_init!(init);
fn init(_: Address) {
    println!("Start");
    let mut state = State::fetch();

    loop {
        match handle_message(&mut state) {
            Ok(_) => {}
            Err(e) => println!("got error while handling message: {e:?}"),
        }
    }
}
