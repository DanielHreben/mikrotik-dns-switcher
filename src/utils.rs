use tiny_http::{Header, Response};

/// Creates a JSON response with the specified body and status code
pub fn json_response(body: &str, status_code: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(body);

    // Add JSON content type header
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response = response.with_header(header);
    }

    // Set status code if not 200
    if status_code != 200 {
        response = response.with_status_code(status_code);
    }

    response
}

/// Extract client IP from X-Real-IP header for reverse proxy setups
pub fn get_custom_ip_from_headers(headers: &[Header]) -> Option<String> {
    for header in headers {
        let field_name = header.field.as_str();

        if field_name == "X-Real-IP" || field_name == "x-real-ip" {
            return Some(header.value.as_str().to_string());
        }
    }

    None
}
