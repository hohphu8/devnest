use crate::core::service_manager;
use crate::error::AppError;
use crate::models::mobile_preview::{MobilePreviewStatus, ProjectMobilePreviewState};
use crate::models::project::Project;
use crate::models::service::{ServiceName, ServiceStatus};
use crate::state::{AppState, MobilePreviewSession};
use crate::storage::repositories::now_iso;
use reqwest::blocking::Client;
use reqwest::header::{HOST, HeaderName, HeaderValue};
use reqwest::{Method, Url};
use rusqlite::Connection;
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

const MAX_REQUEST_HEADERS: usize = 64 * 1024;
const MAX_REQUEST_BODY: usize = 10 * 1024 * 1024;

#[derive(Clone)]
struct PreviewProxyConfig {
    domain: String,
    host_header: String,
    upstream_origin: String,
    proxy_url: String,
    proxy_authority: String,
    local_project_url: String,
    ssl_enabled: bool,
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    original_host: Option<String>,
}

struct ProxyResponse {
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn mobile_preview_lock_error(message: &str) -> AppError {
    AppError::new_validation("MOBILE_PREVIEW_STATE_LOCK_FAILED", message)
}

fn local_project_url(project: &Project) -> String {
    if project.ssl_enabled {
        format!("https://{}", project.domain)
    } else {
        format!("http://{}", project.domain)
    }
}

fn stopped_state(
    project: &Project,
    details: Option<String>,
) -> Result<ProjectMobilePreviewState, AppError> {
    Ok(ProjectMobilePreviewState {
        project_id: project.id.clone(),
        status: MobilePreviewStatus::Stopped,
        local_project_url: local_project_url(project),
        lan_ip: None,
        port: None,
        proxy_url: None,
        qr_url: None,
        updated_at: now_iso()?,
        details,
    })
}

fn error_state_from_session(
    project: &Project,
    current: &ProjectMobilePreviewState,
    details: impl Into<String>,
) -> Result<ProjectMobilePreviewState, AppError> {
    Ok(ProjectMobilePreviewState {
        project_id: project.id.clone(),
        status: MobilePreviewStatus::Error,
        local_project_url: current.local_project_url.clone(),
        lan_ip: current.lan_ip.clone(),
        port: current.port,
        proxy_url: current.proxy_url.clone(),
        qr_url: current.qr_url.clone(),
        updated_at: now_iso()?,
        details: Some(details.into()),
    })
}

fn project_service_name(project: &Project) -> ServiceName {
    match project.server_type {
        crate::models::project::ServerType::Apache => ServiceName::Apache,
        crate::models::project::ServerType::Nginx => ServiceName::Nginx,
    }
}

fn ensure_project_service_running(
    connection: &Connection,
    state: &AppState,
    project: &Project,
) -> Result<Option<u16>, AppError> {
    let service = project_service_name(project);
    let service_state = service_manager::get_service_status(connection, state, service.clone())?;
    if !matches!(service_state.status, ServiceStatus::Running) {
        return Err(AppError::new_validation(
            "MOBILE_PREVIEW_UPSTREAM_UNAVAILABLE",
            format!(
                "Start {} for this project first, then try Mobile Preview again.",
                service.display_name()
            ),
        ));
    }

    Ok(service_state
        .port
        .and_then(|port| u16::try_from(port).ok())
        .or_else(|| service.default_port()))
}

fn is_private_lan_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    if matches!(octets, [10, _, _, _] | [192, 168, _, _]) {
        return true;
    }

    matches!(octets, [172, second, _, _] if (16..=31).contains(&second))
}

fn resolve_lan_ipv4() -> Result<Ipv4Addr, AppError> {
    for target in [
        "192.168.0.1:80",
        "10.0.0.1:80",
        "172.16.0.1:80",
        "1.1.1.1:80",
    ] {
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => socket,
            Err(_) => continue,
        };

        if socket.connect(target).is_err() {
            continue;
        }

        let local = match socket.local_addr() {
            Ok(local) => local,
            Err(_) => continue,
        };

        if let IpAddr::V4(ip) = local.ip() {
            if is_private_lan_ipv4(&ip) && !ip.is_loopback() {
                return Ok(ip);
            }
        }
    }

    Err(AppError::new_validation(
        "MOBILE_PREVIEW_LAN_IP_UNAVAILABLE",
        "DevNest could not find a private LAN IPv4 address. Connect to a Wi-Fi or Ethernet network, then try Mobile Preview again.",
    ))
}

fn build_proxy_response(
    config: &PreviewProxyConfig,
    response: reqwest::blocking::Response,
) -> Result<ProxyResponse, AppError> {
    let status = response.status();
    let response_headers = response.headers().clone();
    let content_type = response_headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let rewrite_text = content_type
        .as_deref()
        .map(should_rewrite_response_body)
        .unwrap_or(false);

    let mut body = response
        .bytes()
        .map_err(|error| {
            AppError::with_details(
                "MOBILE_PREVIEW_UPSTREAM_UNAVAILABLE",
                "DevNest received an incomplete response from the local project server.",
                error.to_string(),
            )
        })?
        .to_vec();

    if rewrite_text {
        let text = String::from_utf8_lossy(&body);
        body = rewrite_response_text(&text, config).into_bytes();
    }

    let mut headers = Vec::new();
    for (name, value) in &response_headers {
        if should_skip_response_header(name.as_str()) {
            continue;
        }

        let Ok(mut rendered) = value.to_str().map(str::to_string) else {
            continue;
        };

        match name.as_str().to_ascii_lowercase().as_str() {
            "location" | "refresh" => {
                rendered = rewrite_response_text(&rendered, config);
            }
            "set-cookie" => {
                rendered = rewrite_set_cookie_domain(&rendered, &config.domain, false);
            }
            _ => {}
        }

        headers.push((name.as_str().to_string(), rendered));
    }

    headers.push(("Content-Length".to_string(), body.len().to_string()));
    headers.push(("Connection".to_string(), "close".to_string()));

    Ok(ProxyResponse {
        status_code: status.as_u16(),
        reason_phrase: status.canonical_reason().unwrap_or("OK").to_string(),
        headers,
        body,
    })
}

fn should_skip_request_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
            | "accept-encoding"
    )
}

fn should_skip_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "content-encoding"
    )
}

fn should_rewrite_response_body(content_type: &str) -> bool {
    let normalized = content_type.to_ascii_lowercase();
    normalized.starts_with("text/")
        || normalized.contains("json")
        || normalized.contains("javascript")
        || normalized.contains("xml")
        || normalized.contains("svg")
        || normalized.contains("x-www-form-urlencoded")
}

fn rewrite_response_text(value: &str, config: &PreviewProxyConfig) -> String {
    let proxy_base = config.proxy_url.trim_end_matches('/');
    let mut next = value.replace(&format!("https://{}", config.domain), proxy_base);
    next = next.replace(&format!("http://{}", config.domain), proxy_base);
    next = next.replace(
        &format!("//{}", config.domain),
        &format!("//{}", config.proxy_authority),
    );
    next
}

fn rewrite_set_cookie_domain(value: &str, domain: &str, allow_secure_cookie: bool) -> String {
    value
        .split(';')
        .map(str::trim)
        .filter(|segment| {
            let lower = segment.to_ascii_lowercase();
            if !lower.starts_with("domain=") {
                return true;
            }

            let Some(cookie_domain) = segment.split('=').nth(1) else {
                return true;
            };
            let normalized = cookie_domain.trim().trim_start_matches('.');
            !normalized.eq_ignore_ascii_case(domain)
        })
        .filter(|segment| allow_secure_cookie || !segment.eq_ignore_ascii_case("secure"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn write_proxy_response(stream: &mut TcpStream, response: ProxyResponse) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {} {}\r\n",
        response.status_code, response.reason_phrase
    )?;
    for (name, value) in response.headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")?;
    stream.write_all(&response.body)?;
    stream.flush()
}

fn write_http_error_response(
    stream: &mut TcpStream,
    status_code: u16,
    title: &str,
    message: &str,
) -> std::io::Result<()> {
    let body = format!("{title}\n\n{message}\n");
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        title,
        body.len(),
        body
    )?;
    stream.flush()
}

fn normalize_request_target(target: &str) -> String {
    if (target.starts_with("http://") || target.starts_with("https://"))
        && let Ok(url) = Url::parse(target)
    {
        let mut normalized = url.path().to_string();
        if normalized.is_empty() {
            normalized.push('/');
        }
        if let Some(query) = url.query() {
            normalized.push('?');
            normalized.push_str(query);
        }
        return normalized;
    }

    if target.is_empty() {
        "/".to_string()
    } else {
        target.to_string()
    }
}

fn read_socket_chunk(stream: &mut TcpStream, chunk: &mut [u8]) -> Result<usize, AppError> {
    match stream.read(chunk) {
        Ok(bytes_read) => Ok(bytes_read),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::WouldBlock
                    | ErrorKind::TimedOut
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::UnexpectedEof
            ) =>
        {
            Err(AppError::with_details(
                "MOBILE_PREVIEW_REQUEST_INVALID",
                "DevNest did not receive a complete HTTP request from the phone browser.",
                error.to_string(),
            ))
        }
        Err(error) => Err(AppError::with_details(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "DevNest could not read the incoming Mobile Preview request.",
            error.to_string(),
        )),
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<Option<ParsedRequest>, AppError> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let mut header_end = None;

    while header_end.is_none() {
        let bytes_read = read_socket_chunk(stream, &mut chunk)?;
        if bytes_read == 0 {
            if buffer.is_empty() {
                return Ok(None);
            }
            break;
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);
        if buffer.len() > MAX_REQUEST_HEADERS {
            return Err(AppError::new_validation(
                "MOBILE_PREVIEW_REQUEST_INVALID",
                "The incoming mobile preview request headers are too large.",
            ));
        }

        header_end = buffer
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4);
        if header_end.is_none() {
            header_end = buffer
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2);
        }
    }

    let header_end = header_end.ok_or_else(|| {
        AppError::new_validation(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "DevNest could not parse the incoming mobile preview request.",
        )
    })?;

    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let normalized_header_text = header_text.replace("\r\n", "\n");
    let mut lines = normalized_header_text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty());
    let request_line = lines.next().ok_or_else(|| {
        AppError::new_validation(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "The mobile preview request line is missing.",
        )
    })?;
    let request_parts = request_line.split_whitespace().collect::<Vec<_>>();
    if request_parts.len() < 2 {
        return Err(AppError::new_validation(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "The mobile preview request line is invalid.",
        ));
    }

    let mut headers = Vec::new();
    let mut original_host = None;
    let mut content_length = 0usize;
    let mut transfer_encoding_chunked = false;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let header_name = name.trim().to_string();
        let header_value = value.trim().to_string();
        if header_name.eq_ignore_ascii_case("host") {
            original_host = Some(header_value.clone());
        }
        if header_name.eq_ignore_ascii_case("content-length") {
            content_length = header_value.parse::<usize>().unwrap_or(0);
        }
        if header_name.eq_ignore_ascii_case("transfer-encoding")
            && header_value.to_ascii_lowercase().contains("chunked")
        {
            transfer_encoding_chunked = true;
        }
        headers.push((header_name, header_value));
    }

    if transfer_encoding_chunked {
        return Err(AppError::new_validation(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "Chunked request uploads are not supported in Mobile Preview yet.",
        ));
    }

    if content_length > MAX_REQUEST_BODY {
        return Err(AppError::new_validation(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "The incoming mobile preview request body is too large.",
        ));
    }

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let bytes_read = read_socket_chunk(stream, &mut chunk)?;
        if bytes_read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..bytes_read]);
        if body.len() > MAX_REQUEST_BODY {
            return Err(AppError::new_validation(
                "MOBILE_PREVIEW_REQUEST_INVALID",
                "The incoming mobile preview request body is too large.",
            ));
        }
    }

    body.truncate(content_length);

    Ok(Some(ParsedRequest {
        method: request_parts[0].to_string(),
        target: normalize_request_target(request_parts[1]),
        headers,
        body,
        original_host,
    }))
}

fn forward_request(
    request: ParsedRequest,
    client_addr: SocketAddr,
    config: &PreviewProxyConfig,
) -> Result<ProxyResponse, AppError> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|error| {
            AppError::with_details(
                "MOBILE_PREVIEW_START_FAILED",
                "DevNest could not prepare the mobile preview proxy client.",
                error.to_string(),
            )
        })?;

    let upstream_url = Url::parse(&config.upstream_origin)
        .and_then(|base| base.join(&request.target))
        .map_err(|error| {
            AppError::with_details(
                "MOBILE_PREVIEW_REQUEST_INVALID",
                "DevNest could not build the local upstream URL for this request.",
                error.to_string(),
            )
        })?;

    let method = Method::from_bytes(request.method.as_bytes()).map_err(|error| {
        AppError::with_details(
            "MOBILE_PREVIEW_REQUEST_INVALID",
            "The mobile preview request method is not supported.",
            error.to_string(),
        )
    })?;

    let mut builder = client.request(method, upstream_url);
    for (name, value) in &request.headers {
        if should_skip_request_header(name) {
            continue;
        }

        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(value) else {
            continue;
        };
        builder = builder.header(header_name, header_value);
    }

    builder = builder
        .header(HOST, config.host_header.as_str())
        .header("accept-encoding", "identity")
        .header("connection", "close")
        .header("x-forwarded-for", client_addr.ip().to_string())
        .header(
            "x-forwarded-host",
            request
                .original_host
                .as_deref()
                .unwrap_or(config.proxy_authority.as_str()),
        )
        .header(
            "x-forwarded-proto",
            if config.ssl_enabled { "https" } else { "http" },
        );

    if !request.body.is_empty() {
        builder = builder.body(request.body);
    }

    let response = builder.send().map_err(|error| {
        AppError::with_details(
            "MOBILE_PREVIEW_UPSTREAM_UNAVAILABLE",
            "DevNest could not reach the local project server behind Mobile Preview.",
            error.to_string(),
        )
    })?;

    build_proxy_response(config, response)
}

fn handle_preview_connection(
    mut stream: TcpStream,
    client_addr: SocketAddr,
    config: PreviewProxyConfig,
) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let request = match read_http_request(&mut stream) {
        Ok(Some(request)) => request,
        Ok(None) => return,
        Err(error) => {
            let _ = write_http_error_response(&mut stream, 400, "Bad Request", &error.message);
            return;
        }
    };

    match forward_request(request, client_addr, &config) {
        Ok(response) => {
            let _ = write_proxy_response(&mut stream, response);
        }
        Err(error) => {
            let _ = write_http_error_response(&mut stream, 502, "Bad Gateway", &error.message);
        }
    }
}

fn run_preview_listener(
    listener: TcpListener,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    config: PreviewProxyConfig,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, client_addr)) => {
                let config = config.clone();
                thread::spawn(move || handle_preview_connection(stream, client_addr, config));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }
}

pub fn get_preview_state(
    state: &AppState,
    project: &Project,
) -> Result<Option<ProjectMobilePreviewState>, AppError> {
    let mut sessions = state.project_mobile_previews.lock().map_err(|_| {
        mobile_preview_lock_error("DevNest could not read the current mobile preview state.")
    })?;

    let finished = sessions
        .get(&project.id)
        .and_then(|session| session.worker.as_ref())
        .map(|worker| worker.is_finished())
        .unwrap_or(false);

    if finished && let Some(mut session) = sessions.remove(&project.id) {
        if let Some(worker) = session.worker.take() {
            let _ = worker.join();
        }
        return Ok(Some(error_state_from_session(
            project,
            &session.state,
            "The mobile preview listener stopped unexpectedly. Start it again to continue using the QR preview.",
        )?));
    }

    Ok(sessions
        .get(&project.id)
        .map(|session| session.state.clone()))
}

pub fn start_preview(
    connection: &Connection,
    state: &AppState,
    project: &Project,
) -> Result<ProjectMobilePreviewState, AppError> {
    if let Some(existing) = get_preview_state(state, project)? {
        if matches!(
            existing.status,
            MobilePreviewStatus::Starting | MobilePreviewStatus::Running
        ) {
            return Ok(existing);
        }
    }

    let lan_ip = resolve_lan_ipv4()?;
    let upstream_port = ensure_project_service_running(connection, state, project)?;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|error| {
        AppError::with_details(
            "MOBILE_PREVIEW_START_FAILED",
            "DevNest could not bind a temporary LAN port for Mobile Preview.",
            error.to_string(),
        )
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        AppError::with_details(
            "MOBILE_PREVIEW_START_FAILED",
            "DevNest could not prepare the Mobile Preview listener.",
            error.to_string(),
        )
    })?;
    let bind_address = listener.local_addr().map_err(|error| {
        AppError::with_details(
            "MOBILE_PREVIEW_START_FAILED",
            "DevNest could not read the bound Mobile Preview port.",
            error.to_string(),
        )
    })?;
    let proxy_url = format!("http://{}:{}/", lan_ip, bind_address.port());
    let upstream_origin = if project.ssl_enabled {
        format!("https://{}/", project.domain)
    } else {
        format!("http://127.0.0.1:{}/", upstream_port.unwrap_or(80))
    };
    let config = PreviewProxyConfig {
        domain: project.domain.clone(),
        host_header: project.domain.clone(),
        upstream_origin,
        proxy_authority: format!("{}:{}", lan_ip, bind_address.port()),
        proxy_url: proxy_url.clone(),
        local_project_url: local_project_url(project),
        ssl_enabled: project.ssl_enabled,
    };

    let next_state = ProjectMobilePreviewState {
        project_id: project.id.clone(),
        status: MobilePreviewStatus::Running,
        local_project_url: config.local_project_url.clone(),
        lan_ip: Some(lan_ip.to_string()),
        port: Some(bind_address.port()),
        proxy_url: Some(proxy_url.clone()),
        qr_url: Some(proxy_url),
        updated_at: now_iso()?,
        details: Some(format!(
            "LAN preview is running for {}. Scan the QR code from a phone on the same Wi-Fi network.",
            project.domain
        )),
    };

    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_shutdown = shutdown.clone();
    let worker = thread::spawn(move || run_preview_listener(listener, worker_shutdown, config));

    state
        .project_mobile_previews
        .lock()
        .map_err(|_| {
            mobile_preview_lock_error("DevNest could not store the new mobile preview session.")
        })?
        .insert(
            project.id.clone(),
            MobilePreviewSession {
                state: next_state.clone(),
                bind_address,
                shutdown,
                worker: Some(worker),
            },
        );

    Ok(next_state)
}

pub fn stop_preview(
    state: &AppState,
    project: &Project,
) -> Result<ProjectMobilePreviewState, AppError> {
    let session = state
        .project_mobile_previews
        .lock()
        .map_err(|_| {
            mobile_preview_lock_error("DevNest could not stop the mobile preview session.")
        })?
        .remove(&project.id);

    let Some(mut session) = session else {
        return stopped_state(
            project,
            Some("Mobile preview is already stopped for this project.".to_string()),
        );
    };

    session.shutdown.store(true, Ordering::Relaxed);
    let _ = TcpStream::connect(session.bind_address);
    if let Some(worker) = session.worker.take() {
        let _ = worker.join();
    }

    stopped_state(
        project,
        Some(
            "Mobile preview stopped. Start it again whenever you need a fresh QR session."
                .to_string(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        PreviewProxyConfig, is_private_lan_ipv4, normalize_request_target, rewrite_response_text,
        rewrite_set_cookie_domain, should_rewrite_response_body,
    };
    use std::net::Ipv4Addr;

    fn config() -> PreviewProxyConfig {
        PreviewProxyConfig {
            domain: "shop.test".to_string(),
            host_header: "shop.test".to_string(),
            upstream_origin: "http://127.0.0.1:80/".to_string(),
            proxy_url: "http://192.168.1.5:50321/".to_string(),
            proxy_authority: "192.168.1.5:50321".to_string(),
            local_project_url: "https://shop.test".to_string(),
            ssl_enabled: false,
        }
    }

    #[test]
    fn recognizes_private_lan_ipv4_ranges() {
        assert!(is_private_lan_ipv4(&Ipv4Addr::new(192, 168, 1, 25)));
        assert!(is_private_lan_ipv4(&Ipv4Addr::new(10, 24, 8, 99)));
        assert!(is_private_lan_ipv4(&Ipv4Addr::new(172, 20, 1, 15)));
        assert!(!is_private_lan_ipv4(&Ipv4Addr::new(127, 0, 0, 1)));
        assert!(!is_private_lan_ipv4(&Ipv4Addr::new(8, 8, 8, 8)));
    }

    #[test]
    fn normalizes_absolute_form_request_targets() {
        assert_eq!(
            normalize_request_target("https://shop.test/login?redirect=1"),
            "/login?redirect=1"
        );
        assert_eq!(
            normalize_request_target("/assets/app.css"),
            "/assets/app.css"
        );
    }

    #[test]
    fn rewrites_project_domain_references_to_proxy_url() {
        let rewritten = rewrite_response_text(
            r#"<a href="https://shop.test/login">Login</a><img src="//shop.test/logo.svg">"#,
            &config(),
        );

        assert!(rewritten.contains(r#"href="http://192.168.1.5:50321/login""#));
        assert!(rewritten.contains(r#"src="//192.168.1.5:50321/logo.svg""#));
    }

    #[test]
    fn strips_matching_cookie_domain() {
        assert_eq!(
            rewrite_set_cookie_domain(
                "laravel_session=abc; Path=/; Domain=shop.test; HttpOnly; SameSite=Lax",
                "shop.test",
                false,
            ),
            "laravel_session=abc; Path=/; HttpOnly; SameSite=Lax"
        );
    }

    #[test]
    fn strips_secure_cookie_flag_for_http_proxy_mode() {
        assert_eq!(
            rewrite_set_cookie_domain(
                "laravel_session=abc; Path=/; Domain=shop.test; Secure; HttpOnly",
                "shop.test",
                false,
            ),
            "laravel_session=abc; Path=/; HttpOnly"
        );
    }

    #[test]
    fn recognizes_textual_content_types_for_rewrite() {
        assert!(should_rewrite_response_body("text/html; charset=utf-8"));
        assert!(should_rewrite_response_body("application/json"));
        assert!(!should_rewrite_response_body("image/png"));
    }
}
