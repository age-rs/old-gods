use wasm_bindgen::prelude::*;
use web_sys::{Request, RequestMode, RequestInit, Response};
use mogwai::prelude::*;
use serde::de::DeserializeOwned;
use serde_json;

async fn request_to_text(req:Request) -> Result<String, String> {
  let resp:Response =
    JsFuture::from(
      window()
        .fetch_with_request(&req)
    )
    .await
    .map_err(|_| "request failed".to_string())?
    .dyn_into()
    .map_err(|_| "response is malformed")?;
  let text:String =
    JsFuture::from(
      resp
        .text()
        .map_err(|_| "could not get response text")?
    )
    .await
    .map_err(|_| "getting text failed")?
    .as_string()
    .ok_or("couldn't get text as string".to_string())?;
  Ok(text)
}


pub async fn from_json<T: DeserializeOwned>(url:&str) -> Result<T, String> {
  let mut opts =
    RequestInit::new();
  opts.method("GET");
  opts.mode(RequestMode::Cors);

  let req =
    Request::new_with_str_and_init(
      url,
      &opts
    )
    .unwrap();

  let result =
    request_to_text(req).await?;
  serde_json::from_str(&result)
    .map_err(|e| format!("{}", e))
}
