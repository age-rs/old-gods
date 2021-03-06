use serde::de::DeserializeOwned;

use std::{future::Future, pin::Pin};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, Request, RequestInit, RequestMode, Response};

async fn request_to_text(req: Request) -> Result<String, String> {
    let window = window().ok_or("could not get window")?;
    let resp: Response = JsFuture::from(window.fetch_with_request(&req))
        .await
        .map_err(|_| "request failed".to_string())?
        .dyn_into()
        .map_err(|_| "response is malformed")?;
    let text: String = JsFuture::from(resp.text().map_err(|_| "could not get response text")?)
        .await
        .map_err(|_| "getting text failed")?
        .as_string()
        .ok_or_else(|| "couldn't get text as string".to_string())?;
    Ok(text)
}

pub fn from_url(url: &str) -> Pin<Box<dyn Future<Output = Result<String, String>>>> {
    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let req = Request::new_with_str_and_init(url, &opts).unwrap();

    Box::pin(async move { request_to_text(req).await })
}

pub async fn _from_json<T: DeserializeOwned>(url: &str) -> Result<T, String> {
    let result: String = from_url(url).await?;
    serde_json::from_str(&result).map_err(|e| format!("{}", e))
}
