mod common;
mod config;
mod proxy;

use crate::config::Config;
use crate::proxy::*;

use std::collections::HashMap;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use serde_json::json;
use uuid::Uuid;
use worker::*;
use once_cell::sync::Lazy;
use regex::Regex;

static PROXYIP_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^.+-\d+$").unwrap());
static PROXYKV_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([A-Z]{2})").unwrap());

#[event(fetch)]
async fn main(req: Request, env: Env, _: Context) -> Result<Response> {
    let uuid = env
        .var("UUID")
        .map(|x| Uuid::parse_str(&x.to_string()).unwrap_or_default())?;
    let host = req.url()?.host().map(|x| x.to_string()).unwrap_or_default();
    let main_page_url = env.var("MAIN_PAGE_URL").map(|x| x.to_string()).unwrap();
    let sub_page_url = env.var("SUB_PAGE_URL").map(|x| x.to_string()).unwrap();
    let link_page_url = env.var("LINK_PAGE_URL").map(|x| x.to_string()).unwrap();
    let converter_page_url = env.var("CONVERTER_PAGE_URL").map(|x| x.to_string()).unwrap();

    let config = Config { 
        uuid, 
        host: host.clone(), 
        proxy_addr: host, 
        proxy_port: 443, 
        main_page_url, 
        sub_page_url,
        link_page_url,
        converter_page_url,
    };

    // Get the URL path to determine if it's a resource request
    let url = req.url()?;
    let path = url.path();

    // Check if the request is for a CSS, JS, or image file
    if path.starts_with("/css/") {
        return handle_css_file(req, env).await;
    } else if path.starts_with("/js/") {
        return handle_js_file(req, env).await;
    } else if path.starts_with("/images/") {
        return handle_image_file(req, env).await;
    }

    // For other routes, use the router
    Router::with_data(config)
        .on_async("/", fe)
        .on_async("/index.html", fe)
        .on_async("/sub", sub)
        .on_async("/sub.html", sub)
        .on_async("/link", link)
        .on_async("/link.html", link)
        .on_async("/converter", converter)
        .on_async("/converter.html", converter)
        .on_async("/:proxyip", tunnel)
        .on_async("/Inconigto-Mode/:proxyip", tunnel)
        .run(req, env)
        .await
}

// Handler for CSS files
async fn handle_css_file(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    
    // Extract the CSS filename from the path
    let filename = path.strip_prefix("/css/").unwrap_or("");
    
    // Get the CSS file from KV storage
    let kv = env.kv("CSS_FILES")?;
    match kv.get(filename).text().await? {
        Some(css) => {
            // Create a response with the CSS content and appropriate headers
            let mut headers = Headers::new();
            headers.set("Content-Type", "text/css")?;
            Ok(Response::ok(css)?.with_headers(headers))
        },
        None => Response::error("CSS file not found", 404)
    }
}

// Handler for JS files
async fn handle_js_file(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    
    // Extract the JS filename from the path
    let filename = path.strip_prefix("/js/").unwrap_or("");
    
    // Get the JS file from KV storage
    let kv = env.kv("JS_FILES")?;
    match kv.get(filename).text().await? {
        Some(js) => {
            // Create a response with the JS content and appropriate headers
            let mut headers = Headers::new();
            headers.set("Content-Type", "application/javascript")?;
            Ok(Response::ok(js)?.with_headers(headers))
        },
        None => Response::error("JavaScript file not found", 404)
    }
}

// Handler for image files
async fn handle_image_file(req: Request, env: Env) -> Result<Response> {
    let url = req.url()?;
    let path = url.path();
    
    // Extract the image filename from the path
    let filename = path.strip_prefix("/images/").unwrap_or("");
    
    // Get the image file from KV storage
    let kv = env.kv("IMAGE_FILES")?;
    match kv.get(filename).bytes().await? {
        Some(image_data) => {
            // Create a response with the image content and appropriate headers
            let mut headers = Headers::new();
            
            // Set content type based on file extension
            if filename.ends_with(".png") {
                headers.set("Content-Type", "image/png")?;
            } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                headers.set("Content-Type", "image/jpeg")?;
            } else if filename.ends_with(".svg") {
                headers.set("Content-Type", "image/svg+xml")?;
            } else if filename.ends_with(".gif") {
                headers.set("Content-Type", "image/gif")?;
            } else {
                headers.set("Content-Type", "application/octet-stream")?;
            }
            
            Ok(Response::from_bytes(image_data)?.with_headers(headers))
        },
        None => Response::error("Image file not found", 404)
    }
}

async fn get_response_from_url(url: String) -> Result<Response> {
    let req = Fetch::Url(Url::parse(url.as_str())?);
    let mut res = req.send().await?;
    Response::from_html(res.text().await?)
}

// Modified to serve HTML from KV if available, otherwise fallback to URL
async fn fe(_: Request, cx: RouteContext<Config>) -> Result<Response> {
    // Try to get HTML from KV storage first
    if let Ok(kv) = cx.env.kv("HTML_FILES") {
        if let Ok(Some(html)) = kv.get("index.html").text().await {
            return Response::from_html(html);
        }
    }
    
    // Fallback to URL if KV storage fails or file not found
    get_response_from_url(cx.data.main_page_url).await
}

// Modified to serve HTML from KV if available, otherwise fallback to URL
async fn sub(_: Request, cx: RouteContext<Config>) -> Result<Response> {
    // Try to get HTML from KV storage first
    if let Ok(kv) = cx.env.kv("HTML_FILES") {
        if let Ok(Some(html)) = kv.get("sub.html").text().await {
            return Response::from_html(html);
        }
    }
    
    // Fallback to URL if KV storage fails or file not found
    get_response_from_url(cx.data.sub_page_url).await
}

// Modified to serve HTML from KV if available, otherwise fallback to URL
async fn link(_: Request, cx: RouteContext<Config>) -> Result<Response> {
    // Try to get HTML from KV storage first
    if let Ok(kv) = cx.env.kv("HTML_FILES") {
        if let Ok(Some(html)) = kv.get("link.html").text().await {
            return Response::from_html(html);
        }
    }
    
    // Fallback to URL if KV storage fails or file not found
    get_response_from_url(cx.data.link_page_url).await
}

// Modified to serve HTML from KV if available, otherwise fallback to URL
async fn converter(_: Request, cx: RouteContext<Config>) -> Result<Response> {
    // Try to get HTML from KV storage first
    if let Ok(kv) = cx.env.kv("HTML_FILES") {
        if let Ok(Some(html)) = kv.get("converter.html").text().await {
            return Response::from_html(html);
        }
    }
    
    // Fallback to URL if KV storage fails or file not found
    get_response_from_url(cx.data.converter_page_url).await
}

async fn tunnel(req: Request, mut cx: RouteContext<Config>) -> Result<Response> {
    let mut proxyip = cx.param("proxyip").unwrap().to_string();
    if PROXYKV_PATTERN.is_match(&proxyip)  {
        let kvid_list: Vec<String> = proxyip.split(",").map(|s| s.to_string()).collect();
        let kv = cx.kv("SIREN")?;
        let mut proxy_kv_str = kv.get("proxy_kv").text().await?.unwrap_or("".to_string());
        let mut rand_buf = [0u8, 1];
        getrandom::getrandom(&mut rand_buf).expect("failed generating random number");

        if proxy_kv_str.len() == 0 {
            console_log!("getting proxy kv from github...");
            let req = Fetch::Url(Url::parse("https://raw.githubusercontent.com/FoolVPN-ID/Nautica/refs/heads/main/kvProxyList.json")?);
            let mut res = req.send().await?;
            if res.status_code() == 200 {
                proxy_kv_str = res.text().await?.to_string();
                kv.put("proxy_kv", &proxy_kv_str)?.expiration_ttl(60 * 60 * 24).execute().await?; // 24 hours
            } else {
                return Err(Error::from(format!("error getting proxy kv: {}", res.status_code())));
            }
        }

        let proxy_kv: HashMap<String, Vec<String>> = serde_json::from_str(&proxy_kv_str)?;

        let kv_index = (rand_buf[0] as usize) % kvid_list.len();
        proxyip = kvid_list[kv_index].clone();

        let proxyip_index = (rand_buf[0] as usize) % proxy_kv[&proxyip].len();
        proxyip = proxy_kv[&proxyip][proxyip_index].clone().replace(":", "-");
    }

    if PROXYIP_PATTERN.is_match(&proxyip) {
        if let Some((addr, port_str)) = proxyip.split_once('-') {
            if let Ok(port) = port_str.parse() {
                cx.data.proxy_addr = addr.to_string();
                cx.data.proxy_port = port;
            }
        }
    }

    let upgrade = req.headers().get("Upgrade")?.unwrap_or("".to_string());
    if upgrade == "websocket".to_string() {
        let WebSocketPair { server, client } = WebSocketPair::new()?;
        server.accept()?;

        wasm_bindgen_futures::spawn_local(async move {
            let events = server.events().unwrap();
            if let Err(e) = ProxyStream::new(cx.data, &server, events).process().await {
                console_log!("[tunnel]: {}", e);
            }
        });

        Response::from_websocket(client)
    } else {
        Response::from_html("hi from wasm!")
    }
}
