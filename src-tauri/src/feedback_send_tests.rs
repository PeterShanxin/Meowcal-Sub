use super::*;
use axum::{body::Bytes, http::HeaderMap, routing::post, Json, Router};
use std::io::Read;

#[test]
fn destination_is_a_build_owned_https_origin() {
    let public = age::x25519::Identity::generate().to_public().to_string();
    for origin in [
        "",
        "http://localhost",
        "https://user@example.com",
        "https://example.com/a",
        "https://example.com/?q=foo",
        "https://example.com/#section",
    ] {
        assert!(Destination::new(origin, &public).is_err());
    }
    assert!(Destination::new("https://example.com", "unconfigured").is_err());
    assert_eq!(
        Destination::new("https://example.com", &public)
            .unwrap()
            .endpoint
            .path(),
        "/v1/reports"
    );
}

#[tokio::test]
async fn only_ciphertext_crosses_the_receiver_boundary() {
    let team = age::x25519::Identity::generate();
    let public = team.to_public();
    let id = Uuid::new_v4();
    let caption = "PRIVATE_TEST_CAPTION";
    let description = "PRIVATE_TEST_DESCRIPTION";
    let payload = seal(
        &public,
        description,
        vec![("translation.txt".to_owned(), caption.as_bytes().into())],
    )
    .unwrap();
    let app = Router::new().route(
        "/v1/reports",
        post(move |headers: HeaderMap, body: Bytes| async move {
            assert_eq!(headers["x-report-id"], id.to_string());
            assert_eq!(headers["x-report-type"], "translation");
            assert_eq!(headers["x-app-version"], env!("CARGO_PKG_VERSION"));
            assert_eq!(headers["content-type"], "application/octet-stream");
            assert!(body.starts_with(b"age-encryption.org/v1\n"));
            for private in [caption, description, "translation.txt"] {
                assert!(!body
                    .windows(private.len())
                    .any(|part| part == private.as_bytes()));
            }
            assert!(age::decrypt(&age::x25519::Identity::generate(), &body).is_err());
            let plaintext = age::decrypt(&team, &body).unwrap();
            let mut zip = zip::ZipArchive::new(Cursor::new(plaintext)).unwrap();
            assert_eq!(zip.len(), 2);
            for (name, expected) in [
                ("description.txt", description),
                ("translation.txt", caption),
            ] {
                let mut actual = String::new();
                zip.by_name(name)
                    .unwrap()
                    .read_to_string(&mut actual)
                    .unwrap();
                assert_eq!(actual, expected);
            }
            Json(serde_json::json!({"issueNumber":184}))
        }),
    );
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/reports", socket.local_addr().unwrap())
        .parse()
        .unwrap();
    let server = tokio::spawn(async move { axum::serve(socket, app).await.unwrap() });
    assert_eq!(
        upload(url, id, "translation", payload)
            .await
            .unwrap()
            .issue_number,
        184
    );
    server.abort();
}

#[test]
fn size_and_archive_names_are_bounded_before_upload() {
    let recipient = age::x25519::Identity::generate().to_public();
    for path in ["../private", "local\\file", "C:log", ".hidden"] {
        assert_eq!(
            seal(&recipient, "", vec![(path.into(), vec![])]).unwrap_err(),
            INVALID
        );
    }
    assert_eq!(
        seal(&recipient, "x", vec![("large".into(), vec![0; MAX_BYTES])]).unwrap_err(),
        LARGE
    );
}
