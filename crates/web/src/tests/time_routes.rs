//! Bridge tests for natural-language time: the preview route forwards to
//! `ResolveTime`, and snooze stores exactly the instant the preview chose.

use super::*;
use std::sync::{Arc, Mutex};

async fn serve(socket_path: PathBuf) -> std::net::SocketAddr {
    bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn time_resolve_forwards_input_and_now_and_passes_the_payload_through() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let Request::ResolveTime { input, now, .. } = &request else {
                return Some(Response::error("unexpected request"));
            };
            let now = now
                .unwrap()
                .with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
            let result = mxr_core::natural_time::resolve_time(
                input,
                &now,
                &mxr_core::natural_time::TimePrefs::default(),
            );
            seen_for_ipc.lock().unwrap().push(request.clone());
            Some(Response::Ok {
                data: ResponseData::resolved_time(input, result),
            })
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let client = reqwest::Client::new();

    let json: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/v1/mail/time/resolve?input=fri%203&now=2024-05-07T14:00:00Z"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json["kind"], "ResolvedTime");
    assert_eq!(json["resolution"]["at"], "2024-05-10T15:00:00Z");
    assert_eq!(json["resolution"]["choices"][1]["label"], "03:00");
    assert_eq!(json["resolution"]["spans"][0]["end"], 5);
    assert!(json["error"].is_null());

    let json: serde_json::Value = client
        .get(format!(
            "http://{addr}/api/v1/mail/time/resolve?input=frday&now=2024-05-07T14:00:00Z&time_zone=Europe/London"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(json["resolution"].is_null());
    assert_eq!(json["error"]["kind"], "unrecognized");
    assert_eq!(json["error"]["token"], "frday");

    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[1],
        Request::ResolveTime { time_zone: Some(zone), .. } if zone == "Europe/London"
    ));
    assert!(matches!(
        &seen[0],
        Request::ResolveTime { input, now: Some(_), time_zone: None } if input == "fri 3"
    ));
}

#[tokio::test]
async fn snooze_stores_the_previewed_instant_unchanged() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            seen_for_ipc.lock().unwrap().push(request);
            Some(Response::Ok {
                data: ResponseData::Ack,
            })
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;
    let message_id = MessageId::new();

    // The web sends `choices[n].at` back as `until`; a preview in +01:00 must
    // land on the same instant.
    let response = reqwest::Client::new()
        .post(format!("http://{addr}/api/v1/mail/actions/snooze"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&serde_json::json!({
            "message_id": message_id.to_string(),
            "until": "2099-05-10T15:00:00+01:00",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let seen = seen.lock().unwrap();
    let Request::Snooze { wake_at, .. } = &seen[0] else {
        panic!("expected a snooze request, got {:?}", seen[0]);
    };
    assert_eq!(
        *wake_at,
        Utc.with_ymd_and_hms(2099, 5, 10, 14, 0, 0).unwrap()
    );
}

#[tokio::test]
async fn snooze_presets_come_from_the_daemon_in_the_browser_zone() {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let Request::ResolveTime { input, .. } = &request else {
                return Some(Response::error("unexpected request"));
            };
            let now = Utc.with_ymd_and_hms(2024, 5, 7, 20, 0, 0).unwrap();
            // At 20:00, "tonight" has passed and is left out.
            let result = mxr_core::natural_time::resolve_time(
                input,
                &now,
                &mxr_core::natural_time::TimePrefs::default(),
            );
            seen_for_ipc.lock().unwrap().push(request.clone());
            Some(Response::Ok {
                data: ResponseData::resolved_time(input, result),
            })
        },
        None,
    )
    .await;
    let addr = serve(socket_path).await;

    let json: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "http://{addr}/api/v1/mail/actions/snooze/presets?time_zone=Asia/Tokyo"
        ))
        .bearer_auth(TEST_AUTH_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = json["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|preset| preset["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["tomorrow", "weekend", "monday"]);
    assert_eq!(json["presets"][0]["wakeAt"], "2024-05-08T09:00:00Z");
    assert!(seen.lock().unwrap().iter().all(|request| matches!(
        request,
        Request::ResolveTime { time_zone: Some(zone), .. } if zone == "Asia/Tokyo"
    )));
}
