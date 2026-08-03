use super::*;

#[derive(Debug, Clone)]
struct ValidatedWebUrl {
    url: reqwest::Url,
    host: String,
    addresses: Vec<SocketAddr>,
}

fn validate_web_url(url: &reqwest::Url) -> Result<ValidatedWebUrl, String> {
    if url.scheme() != "https" {
        return Err("Для сайтов и информационных систем разрешён только HTTPS.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL со встроенными учётными данными запрещён.".into());
    }
    if url.fragment().is_some() {
        return Err("Фрагмент URL (#...) не используется для загрузки источника.".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "URL не содержит имя узла.".to_string())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
    {
        return Err("Доступ к локальным адресам запрещён.".into());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let mut addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("Не удалось разрешить адрес: {error}"))?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err("Доступ к локальным, служебным и приватным сетям запрещён.".into());
    }
    Ok(ValidatedWebUrl {
        url: url.clone(),
        host,
        addresses,
    })
}

fn pinned_web_client(validated: &ValidatedWebUrl) -> Result<reqwest::blocking::Client, String> {
    crate::ensure_rustls_crypto_provider();
    reqwest::blocking::Client::builder()
        .https_only(true)
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&validated.host, &validated.addresses)
        .user_agent(concat!("Dokkomplekt-Universal/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())
}

pub fn fetch_web_source(url: &str, workspace: &Path) -> Result<WebIntakeResult, String> {
    let mut current =
        reqwest::Url::parse(url).map_err(|error| format!("Некорректный URL: {error}"))?;
    let mut response = None;
    for redirect_count in 0..=5 {
        let validated = validate_web_url(&current)?;
        let client = pinned_web_client(&validated)?;
        let candidate = client
            .get(validated.url.clone())
            .send()
            .map_err(|error| format!("Не удалось получить источник: {error}"))?;
        if candidate.status().is_redirection() {
            if redirect_count >= 5 {
                return Err("Слишком много HTTPS-перенаправлений.".into());
            }
            let location = candidate
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| "Перенаправление не содержит корректный Location.".to_string())?;
            current = validated
                .url
                .join(location)
                .map_err(|error| format!("Некорректное перенаправление: {error}"))?;
            continue;
        }
        response = Some(candidate);
        break;
    }
    let response = response.ok_or_else(|| "HTTPS-источник не получен.".to_string())?;
    if !response.status().is_success() {
        return Err(format!("Сайт вернул HTTP {}.", response.status()));
    }
    let final_url = response.url().clone();
    validate_web_url(&final_url)?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .trim()
        .to_ascii_lowercase();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_UPLOAD_BYTES as u64)
    {
        return Err("Ответ сайта превышает 100 МБ.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_UPLOAD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Не удалось безопасно прочитать HTTPS-ответ: {error}"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("Ответ сайта превышает 100 МБ.".into());
    }

    let mut warnings = vec![
        "Источник получен извне; критические поля проходят risk gate и межполевые проверки.".into(),
    ];
    let source_text = if is_textual_content_type(&content_type) {
        normalize_web_text(&bytes, &content_type)?
    } else {
        let extension = web_extension(&final_url, &content_type).ok_or_else(|| {
            format!("Тип ответа «{content_type}» пока нельзя безопасно нормализовать.")
        })?;
        let mut session =
            normalize_uploaded_bytes(&format!("web-source.{extension}"), &bytes, workspace)?;
        let normalized = session.take_source()?;
        warnings.extend(normalized.warnings);
        normalized.text
    };
    if source_text.trim().is_empty() {
        return Err("HTTPS-источник не содержит пригодного для обработки текста.".into());
    }
    Ok(WebIntakeResult {
        source_text: normalize_text(&source_text),
        final_url: final_url.to_string(),
        content_type,
        warnings,
        source_sha256: hex::encode(Sha256::digest(&bytes)),
    })
}

fn is_textual_content_type(content_type: &str) -> bool {
    content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
        || content_type.contains("x-www-form-urlencoded")
}

fn normalize_web_text(bytes: &[u8], content_type: &str) -> Result<String, String> {
    let raw = decode_text_bytes(bytes);
    if content_type.contains("html") {
        Ok(html_to_text(&raw))
    } else if content_type.contains("json") {
        serde_json::from_str::<serde_json::Value>(&raw)
            .map(|value| serde_json::to_string_pretty(&value).unwrap_or(raw.clone()))
            .map_err(|error| format!("JSON-ответ повреждён: {error}"))
    } else if content_type.contains("xml") {
        generic_xml_to_text(&raw)
    } else {
        Ok(raw)
    }
}

fn web_extension(url: &reqwest::Url, content_type: &str) -> Option<&'static str> {
    let from_type = match content_type {
        "application/pdf" => Some("pdf"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.ms-word.document.macroenabled.12" => Some("docm"),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.ms-excel" => Some("xls"),
        "application/vnd.oasis.opendocument.spreadsheet" => Some("ods"),
        "application/vnd.oasis.opendocument.text" => Some("odt"),
        "application/rtf" | "text/rtf" => Some("rtf"),
        "message/rfc822" => Some("eml"),
        "application/zip" | "application/x-zip-compressed" => Some("zip"),
        "application/x-7z-compressed" => Some("7z"),
        "application/vnd.rar" | "application/x-rar-compressed" => Some("rar"),
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/tiff" => Some("tiff"),
        "image/bmp" => Some("bmp"),
        "image/webp" => Some("webp"),
        _ => None,
    };
    from_type.or_else(|| {
        let extension = Path::new(url.path()).extension()?.to_str()?;
        supported_extensions()
            .iter()
            .find(|known| extension.eq_ignore_ascii_case(known))
            .copied()
    })
}

pub(super) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            let shared = octets[0] == 100 && (64..=127).contains(&octets[1]);
            let benchmark = octets[0] == 198 && matches!(octets[1], 18 | 19);
            let protocol_assignment = octets[0] == 192 && octets[1] == 0 && octets[2] == 0;
            let deprecated_relay = octets[0] == 192 && octets[1] == 88 && octets[2] == 99;
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_unspecified()
                || ip.is_multicast()
                || shared
                || benchmark
                || protocol_assignment
                || deprecated_relay
                || octets[0] == 0
                || octets[0] >= 240)
        }
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
            let site_local = segments[0] & 0xffc0 == 0xfec0;
            let mapped_forbidden = ip
                .to_ipv4_mapped()
                .is_some_and(|mapped| !is_public_ip(IpAddr::V4(mapped)));
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || documentation
                || site_local
                || mapped_forbidden)
        }
    }
}
